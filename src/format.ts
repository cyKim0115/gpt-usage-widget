import type { TrackUsage } from "./types";

export function formatTrackCaption(track: TrackUsage, now = Date.now()): string {
  if (track.percent === null) return track.value ?? "정보 없음";
  const remaining = formatResetRemaining(track.resetsAt, now);
  const percent = `${Math.round(track.percent)}%`;
  return remaining ? `${percent} · ${remaining}` : percent;
}

export function formatResetRemaining(resetsAt: number | null, now = Date.now()): string {
  if (resetsAt === null || !Number.isFinite(resetsAt)) return "";
  const diff = resetsAt * 1000 - now;
  if (diff <= 0) return "곧 초기화";
  const minutes = Math.max(1, Math.ceil(diff / 60_000));
  if (minutes >= 1440) return `${Math.floor(minutes / 1440)}일 남음`;
  if (minutes >= 60) return `${Math.floor(minutes / 60)}시간 남음`;
  return `${minutes}분 남음`;
}

export function severityClass(percent: number): "" | "warn" | "hot" {
  if (percent >= 90) return "hot";
  if (percent >= 70) return "warn";
  return "";
}

export function formatUpdatedAt(timestamp: number | null): string {
  if (timestamp === null) return "";
  return new Date(timestamp * 1000).toLocaleTimeString([], {
    hour: "2-digit", minute: "2-digit", hour12: false,
  });
}
