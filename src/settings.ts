import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { applyAlwaysOnTop, loadAlwaysOnTop } from "./preferences";
import { loadHiddenTracks, saveTrackVisibility } from "./display-preferences";
import { formatTrackCaption } from "./format";
import { stateLabel, type UsageSnapshot } from "./types";

/** 제목 표시줄과 화면 위아래 여유. 이만큼 빼야 창 아래 끝이 작업 표시줄에 걸리지 않습니다. */
const SCREEN_MARGIN = 80;
/** tauri.conf.json의 minHeight와 같습니다. */
const MIN_HEIGHT = 440;

let autostartEnabled = false;
let isDevBuild = false;
let displayOptionsSignature: string | null = null;

function renderDisplayOptions(snap: UsageSnapshot) {
  const options = $("display-options");
  const hidden = loadHiddenTracks();
  if (snap.tracks.length === 0) {
    displayOptionsSignature = null;
    const hint = document.createElement("p");
    hint.className = "settings-hint";
    hint.textContent = snap.state === "OK"
      ? "이 계정에 표시할 사용량이 없습니다."
      : "사용량을 불러오면 표시 항목을 선택할 수 있습니다.";
    options.replaceChildren(hint);
    return;
  }
  // 스냅샷 재전송 때 DOM을 유지해 키보드 포커스와 스크롤을 보존합니다.
  const signature = JSON.stringify(snap.tracks.map(({ kind, label }) => [kind, label]));
  if (signature === displayOptionsSignature) {
    options.querySelectorAll<HTMLInputElement>("input[data-kind]").forEach((input) => {
      input.checked = !hidden.has(input.dataset.kind!);
    });
    return;
  }
  displayOptionsSignature = signature;
  options.replaceChildren(...snap.tracks.map((track) => {
    const label = document.createElement("label");
    label.className = "settings-toggle";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.dataset.kind = track.kind;
    input.checked = !hidden.has(track.kind);
    const name = document.createElement("span");
    name.textContent = track.label;
    label.append(input, name);
    input.addEventListener("change", async () => {
      try {
        saveTrackVisibility(track.kind, input.checked);
      } catch {
        input.checked = !loadHiddenTracks().has(track.kind);
        setActionStatus("표시 설정을 저장하지 못했습니다. 다시 시도해 주세요.", "error");
        return;
      }
      try {
        await emit("track-visibility-changed");
        setActionStatus("표시 설정을 저장했습니다.", "ok");
      } catch {
        setActionStatus("설정을 저장했습니다. 위젯을 다시 실행하면 적용됩니다.");
      }
    });
    return label;
  }));
}

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

/**
 * 에러 문자열에는 Rust가 옮겨 온 API 응답 본문이 섞일 수 있어서, 마크업을
 * 조립하지 않고 textContent로만 넣습니다.
 */
function showBootError(message: string) {
  const wrap = document.createElement("div");
  wrap.className = "settings-window";
  wrap.style.padding = "24px";

  const title = document.createElement("h1");
  title.textContent = "설정 로드 실패";

  const detail = document.createElement("p");
  detail.className = "settings-hint settings-hint--error";
  detail.textContent = message;

  const close = document.createElement("button");
  close.type = "button";
  close.className = "btn secondary wide";
  close.textContent = "닫기";
  close.addEventListener("click", () => {
    void closeWindow();
  });

  wrap.append(title, detail, close);
  document.body.replaceChildren(wrap);
}

function contentHeight(): number {
  const el = document.querySelector(".settings-window");
  return el ? Math.ceil(el.getBoundingClientRect().height) : 0;
}

/**
 * 화면에 들어가는 만큼만 내용 높이에 창을 맞추고 가운데로 옮깁니다. 내용이 더 길면
 * 스크롤로 넘기고 닫기 버튼은 아래에 붙어 있습니다. 고정 높이로 두면 FHD처럼 세로가
 * 짧은 화면에서 창 아래가 화면 밖으로 나가, 늘리지도 닫지도 못하게 됩니다.
 */
async function fitToScreen() {
  try {
    const win = getCurrentWindow();
    const inner = (await win.innerSize()).toLogical(await win.scaleFactor());
    const maxHeight = Math.max(MIN_HEIGHT, window.screen.availHeight - SCREEN_MARGIN);
    const target = Math.max(MIN_HEIGHT, Math.min(contentHeight(), maxHeight));
    if (Math.abs(target - inner.height) > 1) {
      await win.setSize(new LogicalSize(inner.width, target));
    }
    await win.center();
  } catch {
    /* 크기 조절 실패는 스크롤로 버팁니다 */
  }
}

/**
 * 위에 알림이 떠서 내용이 길어지면, 지금 자리에서 화면 아래 끝까지 남은 만큼만 늘립니다.
 * 사용자가 옮겨 둔 창을 움직이거나 줄이지는 않습니다. 모자라는 만큼은 스크롤입니다.
 */
async function growToContent() {
  try {
    const win = getCurrentWindow();
    const scale = await win.scaleFactor();
    const inner = (await win.innerSize()).toLogical(scale);
    const needed = contentHeight();
    if (needed <= inner.height + 1) return;

    const top = (await win.innerPosition()).toLogical(scale).y;
    const availTop = (window.screen as Screen & { availTop?: number }).availTop ?? 0;
    const room = Math.floor(availTop + window.screen.availHeight - top - 8);
    const target = Math.min(needed, room);
    if (target > inner.height + 1) {
      await win.setSize(new LogicalSize(inner.width, target));
    }
  } catch {
    /* 크기 조절 실패는 스크롤로 버팁니다 */
  }
}

function setActionStatus(message: string, tone: "info" | "ok" | "error" = "info") {
  const el = $("action-status");
  el.textContent = message;
  el.classList.remove("hidden", "settings-status--ok", "settings-status--error");
  if (tone === "ok") el.classList.add("settings-status--ok");
  else if (tone === "error") el.classList.add("settings-status--error");
  void growToContent();
}

function clearActionStatus() {
  $("action-status").classList.add("hidden");
  $("action-status").textContent = "";
}

function badgeClass(state: string): string {
  if (state === "OK") return "settings-badge--ok";
  if (state === "NeedLogin") return "settings-badge--muted";
  return "settings-badge--error";
}

function renderAccount(snap: UsageSnapshot) {
  renderDisplayOptions(snap);
  const activityHint = $("activity-hint");
  activityHint.textContent = snap.activityError ?? "";
  activityHint.classList.toggle("hidden", !snap.activityError);
  const badge = $("connection-badge");
  badge.textContent = stateLabel(snap.state);
  badge.className = `settings-badge ${badgeClass(snap.state)}`;

  $("account-email").textContent = snap.accountEmail ?? "로그인 정보 없음";

  const plan = $("account-plan");
  $("account-usage").replaceChildren();
  if (snap.state === "NeedLogin") {
    plan.textContent = snap.error ?? "Codex에서 ChatGPT 계정으로 로그인한 뒤 새로고침하세요.";
    return;
  }
  if (snap.state !== "OK") {
    plan.textContent = snap.error ?? "사용량을 불러오지 못했습니다.";
    return;
  }
  plan.textContent = snap.planLabel ?? "플랜 정보 없음";
  const usage = $("account-usage");
  usage.replaceChildren(...snap.tracks.map((track) => {
    const line = document.createElement("p");
    line.className = "settings-detail";
    line.textContent = `${track.label}: ${formatTrackCaption(track)}${track.value && track.percent !== null ? ` · ${track.value}` : ""}`;
    return line;
  }));
}

function renderAlwaysOnTop() {
  ($("always-on-top-toggle") as HTMLInputElement).checked = loadAlwaysOnTop();
}

function syncAutostartUi() {
  const toggle = $("autostart-toggle") as HTMLInputElement;
  const hint = $("autostart-hint");

  if (isDevBuild) {
    toggle.checked = false;
    toggle.disabled = true;
    hint.textContent =
      "개발 모드에서는 시작프로그램을 바꿀 수 없습니다. 「시작.bat」으로 설치·실행한 뒤 다시 시도하세요.";
    hint.classList.remove("hidden");
    return;
  }

  toggle.disabled = false;
  toggle.checked = autostartEnabled;
  hint.classList.add("hidden");
}

async function refreshAutostart() {
  isDevBuild = await invoke<boolean>("is_dev_build");
  autostartEnabled = await invoke<boolean>("is_autostart_enabled");
  syncAutostartUi();
}

/**
 * 사용량은 위젯이 혼자 가져옵니다. 설정 창이 직접 get_usage를 부르면 같은 API를
 * 두 창이 각자 두드려 폴링 간격이 사실상 절반이 되므로, 필요할 때 위젯에 요청해
 * 그 결과(usage-updated)를 받아 그립니다.
 */
async function requestUsage() {
  await emit("usage-requested");
}

async function closeWindow() {
  await invoke("close_settings_window");
}

async function refreshView() {
  clearActionStatus();
  renderAlwaysOnTop();
  // 위젯이 주기마다 보내는 사용량을 숨긴 채로도 받아 그려 두어서, 내용 높이는 이미 맞습니다.
  // 값을 다시 읽기 전에 먼저 맞춰야 창이 뜬 뒤 크기가 튀는 시간이 짧습니다.
  await fitToScreen();
  await refreshAutostart();
  await requestUsage();
}

async function onRefresh() {
  clearActionStatus();
  const btn = $("btn-refresh") as HTMLButtonElement;
  btn.disabled = true;
  setActionStatus("사용량을 갱신하고 있습니다…");
  try {
    await emit("refresh-requested");
  } catch (e) {
    setActionStatus(String(e), "error");
    btn.disabled = false;
  }
}

async function boot() {
  try {
    renderAlwaysOnTop();
    await refreshAutostart();

    $("btn-close").addEventListener("click", () => {
      void closeWindow();
    });
    $("btn-refresh").addEventListener("click", () => {
      void onRefresh();
    });

    $("btn-chatgpt-usage").addEventListener("click", async () => {
      const btn = $("btn-chatgpt-usage") as HTMLButtonElement;
      btn.disabled = true;
      clearActionStatus();
      try {
        await invoke("open_chatgpt_usage");
        setActionStatus("브라우저에서 ChatGPT 사용량 화면을 열었습니다.", "ok");
      } catch (e) {
        setActionStatus(String(e), "error");
      } finally {
        btn.disabled = false;
      }
    });

    const alwaysOnTopToggle = $("always-on-top-toggle") as HTMLInputElement;
    alwaysOnTopToggle.addEventListener("change", async () => {
      const enabled = alwaysOnTopToggle.checked;
      try {
        await applyAlwaysOnTop(enabled);
        setActionStatus(
          enabled ? "위젯을 항상 위에 표시합니다." : "위젯을 일반 창처럼 표시합니다.",
          "ok",
        );
      } catch (e) {
        alwaysOnTopToggle.checked = loadAlwaysOnTop();
        setActionStatus(String(e), "error");
      }
    });

    const autostartToggle = $("autostart-toggle") as HTMLInputElement;
    autostartToggle.addEventListener("change", async () => {
      if (isDevBuild) {
        syncAutostartUi();
        return;
      }
      autostartToggle.disabled = true;
      try {
        if (autostartToggle.checked) {
          await invoke("enable_autostart");
        } else {
          await invoke("disable_autostart");
        }
        autostartEnabled = await invoke<boolean>("is_autostart_enabled");
        syncAutostartUi();
        setActionStatus(
          autostartEnabled ? "시작프로그램을 켰습니다." : "시작프로그램을 껐습니다.",
          "ok",
        );
      } catch (e) {
        autostartToggle.checked = autostartEnabled;
        setActionStatus(String(e), "error");
      } finally {
        autostartToggle.disabled = isDevBuild;
      }
    });

    // 숨겨져 있을 때는 다음에 열릴 크기를 미리 맞춰 두고, 떠 있을 때는 사용량 줄이
    // 늘어난 만큼만 키웁니다.
    await listen<UsageSnapshot>("usage-updated", (event) => {
      renderAccount(event.payload);
      void getCurrentWindow()
        .isVisible()
        .catch(() => true)
        .then((visible) => (visible ? growToContent() : fitToScreen()));
      const btn = $("btn-refresh") as HTMLButtonElement;
      if (btn.disabled) {
        btn.disabled = false;
        setActionStatus(event.payload.state === "OK" ? "사용량을 갱신했습니다." : event.payload.error ?? "사용량 갱신에 실패했습니다.", event.payload.state === "OK" ? "ok" : "error");
      }
    });

    // 창은 숨겨둔 채 재사용하므로, 다시 열릴 때마다 값을 새로 읽습니다.
    await listen("settings-open", () => {
      void refreshView();
    });

    window.addEventListener("keydown", (event) => {
      if (event.key === "Escape") void closeWindow();
    });

    await requestUsage();
  } catch (e) {
    showBootError(String(e));
  }
}

void boot();
