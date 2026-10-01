export type TrackUsage = {
  label: string;
  kind: string;
  percent: number;
  resetsAt: number | null;
};

export type UsageSnapshot = {
  state: string;
  accountEmail: string | null;
  planLabel: string | null;
  tracks: TrackUsage[];
  error: string | null;
  fetchedAt: number | null;
};

export function stateLabel(state: string): string {
  switch (state) {
    case "OK":
      return "연결됨";
    case "NeedLogin":
      return "로그인 필요";
    default:
      return "갱신 실패";
  }
}
