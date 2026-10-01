export const HIDDEN_TRACKS_KEY = "gpt-usage-hidden-tracks";

type PreferenceStorage = Pick<Storage, "getItem" | "setItem">;

/** 없거나 손상된 설정은 모든 항목 표시로 복구합니다. */
export function loadHiddenTracks(storage: Pick<Storage, "getItem"> = localStorage): Set<string> {
  try {
    const value: unknown = JSON.parse(storage.getItem(HIDDEN_TRACKS_KEY) ?? "[]");
    return new Set(Array.isArray(value) ? value.filter((kind): kind is string => typeof kind === "string") : []);
  } catch {
    return new Set();
  }
}

/** 한도 순서나 사용량이 달라져도 서버의 항목 ID로 표시 선택을 유지합니다. */
export function saveTrackVisibility(kind: string, visible: boolean, storage: PreferenceStorage = localStorage) {
  const hidden = loadHiddenTracks(storage);
  if (visible) hidden.delete(kind);
  else hidden.add(kind);
  storage.setItem(HIDDEN_TRACKS_KEY, JSON.stringify([...hidden]));
}
