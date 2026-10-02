import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { loadAlwaysOnTop } from "./preferences";
import { HIDDEN_TRACKS_KEY, loadHiddenTracks } from "./display-preferences";
import { formatResetRemaining, formatTrackCaption, formatUpdatedAt, severityClass } from "./format";
import type { TrackUsage, UsageSnapshot } from "./types";

const WINDOW_WIDTH = 340;

let lastSnapshot: UsageSnapshot | null = null;
let refreshInFlight: Promise<void> | null = null;

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function hideContextMenu() {
  $("context-menu").classList.add("hidden");
  $("context-backdrop").classList.add("hidden");
}

/**
 * 메뉴에는 설정과 종료만 둡니다. 창이 트랙 수에 맞춰 160px 안팎까지 줄어들어서,
 * 항목이 늘어나면 메뉴가 창 밖으로 잘려 종료를 누를 수 없게 됩니다. 나머지
 * 항목은 별도 설정 창(settings.html)으로 옮겼습니다.
 */
function showContextMenu(x: number, y: number) {
  const backdrop = $("context-backdrop");
  const menu = $("context-menu");

  backdrop.classList.remove("hidden");
  menu.classList.remove("hidden");

  const menuRect = menu.getBoundingClientRect();
  const maxX = Math.max(8, window.innerWidth - menuRect.width - 8);
  const maxY = Math.max(8, window.innerHeight - menuRect.height - 8);

  menu.style.left = `${Math.min(x, maxX)}px`;
  menu.style.top = `${Math.min(y, maxY)}px`;
}

function buildTrack(track: TrackUsage): HTMLElement {
  const section = document.createElement("section");
  section.className = "track active";
  section.dataset.kind = track.kind;

  const row = document.createElement("div");
  row.className = "row";

  const label = document.createElement("span");
  label.className = "label";
  label.textContent = track.label;
  label.title = track.label;

  const caption = document.createElement("span");
  caption.className = "caption";
  caption.textContent = formatTrackCaption(track);
  caption.title = caption.textContent;

  row.append(label, caption);
  section.append(row);

  // 잔액·예상 횟수는 총량이 없어서 숫자만 표시합니다.
  if (track.percent === null) return section;
  caption.dataset.resetsAt = track.resetsAt === null ? "" : String(track.resetsAt);
  caption.dataset.percent = String(track.percent);

  const bar = document.createElement("div");
  bar.className = "bar";
  bar.setAttribute("role", "progressbar");
  bar.setAttribute("aria-label", track.label);
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", "100");
  bar.setAttribute("aria-valuenow", String(track.percent));

  const fill = document.createElement("div");
  fill.className = "fill";
  const level = severityClass(track.percent);
  if (level) fill.classList.add(level);
  fill.style.width = `${Math.max(0, Math.min(100, track.percent))}%`;

  bar.append(fill);
  section.append(bar);
  if (track.value) {
    const detail = document.createElement("div");
    detail.className = "track-detail";
    detail.textContent = track.value;
    section.append(detail);
  }
  return section;
}

function renderEmpty(message: string): HTMLElement {
  const empty = document.createElement("div");
  empty.className = "empty";
  empty.textContent = message;
  return empty;
}

/** 트랙 수가 플랜마다 달라서, 그린 다음 실제 높이에 창을 맞춥니다. */
async function syncWindowHeight() {
  await new Promise((resolve) => requestAnimationFrame(resolve));
  const height = Math.ceil($("widget").getBoundingClientRect().height);
  if (height <= 0) return;
  try {
    await getCurrentWindow().setSize(new LogicalSize(WINDOW_WIDTH, height));
  } catch {
    /* 크기 조절 실패는 표시를 막지 않습니다 */
  }
}

function render(snap: UsageSnapshot) {
  lastSnapshot = snap;

  const widget = $("widget");
  widget.classList.toggle("error", snap.state !== "OK");

  // 플랜에 따라 트랙 구성이 달라져서, 지금 보는 값이 어느 플랜 기준인지 함께 둡니다.
  // 로그인 전이나 갱신 실패면 빈 문자열이라 헤더가 제목만 남습니다.
  $("plan").textContent = snap.planLabel ?? "";

  const tracks = $("tracks");
  tracks.replaceChildren();

  const hidden = loadHiddenTracks();
  const visibleTracks = snap.tracks.filter((track) => !hidden.has(track.kind));
  if (visibleTracks.length > 0) {
    tracks.append(...visibleTracks.map(buildTrack));
  } else if (snap.state === "OK" && snap.tracks.length > 0) {
    tracks.append(renderEmpty("설정에서 표시할 사용량을 선택하세요"));
  } else if (snap.state === "NeedLogin") {
    tracks.append(renderEmpty("Codex에서 ChatGPT 계정으로 로그인하세요"));
  } else if (snap.state === "OK") {
    tracks.append(renderEmpty("이 계정에 표시할 사용 한도가 없습니다"));
  } else {
    tracks.append(renderEmpty("사용량을 불러오지 못했습니다"));
  }

  const hhmm = formatUpdatedAt(snap.fetchedAt);
  const status = $("status");
  if (snap.state === "OK") {
    status.textContent = `${hhmm} 갱신${snap.activityError ? " · 활동 통계 조회 실패" : ""}`;
  } else if (snap.state === "NeedLogin") {
    status.textContent = "Codex 로그인 필요";
  } else {
    status.textContent = "갱신 실패 · 설정에서 확인하세요";
  }
  status.title = snap.error ?? snap.activityError ?? "";

  void syncWindowHeight();

  // 설정 창이 계정/플랜을 직접 조회하지 않고 이 값을 받아 그립니다.
  void emit("usage-updated", snap).catch(() => {
    /* 설정 창이 없을 수도 있어 실패는 무시합니다 */
  });
}

function refresh(): Promise<void> {
  if (refreshInFlight) return refreshInFlight;
  refreshInFlight = (async () => {
    try {
      render(await invoke<UsageSnapshot>("get_usage"));
    } catch {
      render({ state: "FetchError", accountEmail: null, planLabel: null,
        tracks: [], error: "사용량 조회를 완료하지 못했습니다.", activityError: null, fetchedAt: null });
    } finally {
      refreshInFlight = null;
    }
  })();
  return refreshInFlight;
}

async function boot() {
  const backdrop = $("context-backdrop");
  const menuSettings = $("menu-settings") as HTMLButtonElement;
  const menuQuit = $("menu-quit") as HTMLButtonElement;

  // tauri.conf.json pins the window to always-on-top, so a user who turned it
  // off gets it restored as soon as the webview boots.
  try {
    await invoke("set_always_on_top", { enabled: loadAlwaysOnTop() });
  } catch {
    /* browser preview */
  }

  window.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    showContextMenu(event.clientX, event.clientY);
  });

  backdrop.addEventListener("pointerdown", (event) => {
    if (event.target === backdrop) hideContextMenu();
  });

  $("context-menu").addEventListener("pointerdown", (event) => {
    event.stopPropagation();
  });

  window.addEventListener("blur", () => {
    hideContextMenu();
  });

  window.addEventListener("resize", () => {
    hideContextMenu();
  });

  window.addEventListener("click", () => {
    hideContextMenu();
  });

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") hideContextMenu();
  });

  menuSettings.addEventListener("click", async (event) => {
    event.stopPropagation();
    hideContextMenu();
    try {
      await invoke("open_settings_window");
    } catch (e) {
      window.alert(String(e));
    }
  });

  menuQuit.addEventListener("click", async (event) => {
    event.stopPropagation();
    hideContextMenu();
    await invoke("quit_app");
  });

  // 설정 창이 열릴 때 계정 표시에 쓸 스냅샷을 요구합니다. 아직 한 번도 못
  // 받아왔다면 새로 조회해서, 설정 창이 빈 값으로 남지 않게 합니다.
  await listen("usage-requested", () => {
    if (lastSnapshot) {
      void emit("usage-updated", lastSnapshot);
    } else {
      void refresh();
    }
  });

  // 새로고침은 위젯이 수행합니다. 두 창이 각자 조회하면 폴링 간격이 어긋납니다.
  await listen("refresh-requested", () => {
    void refresh();
  });

  // 표시만 다시 그립니다. 설정 토글은 네트워크 갱신을 일으키지 않습니다.
  const renderPreferences = () => {
    if (lastSnapshot) render(lastSnapshot);
  };
  await listen("track-visibility-changed", renderPreferences);
  window.addEventListener("storage", (event) => {
    if (event.key === HIDDEN_TRACKS_KEY || event.key === null) renderPreferences();
  });

  await refresh();
  let interval = 300_000;
  try {
    interval = await invoke<number>("get_poll_interval_ms");
  } catch {
    /* keep default */
  }
  window.setInterval(() => {
    void refresh();
  }, interval);
  // 리셋까지 남은 시간은 네트워크 조회와 별개로 갱신합니다.
  window.setInterval(() => {
    document.querySelectorAll<HTMLElement>(".caption[data-percent]").forEach((caption) => {
      const remaining = formatResetRemaining(caption.dataset.resetsAt ? Number(caption.dataset.resetsAt) : null);
      const percent = Math.round(Number(caption.dataset.percent));
      caption.textContent = remaining ? `${percent}% · ${remaining}` : `${percent}%`;
      caption.title = caption.textContent;
    });
  }, 60_000);
}

void boot();
