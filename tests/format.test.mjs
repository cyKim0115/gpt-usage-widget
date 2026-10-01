import test from "node:test";
import assert from "node:assert/strict";
import { formatResetRemaining, formatTrackCaption, formatUpdatedAt, severityClass } from "../.test-dist/format.js";

test("reset timestamps are Unix seconds, including missing and elapsed resets", () => {
  assert.equal(formatResetRemaining(null, 0), "");
  assert.equal(formatResetRemaining(NaN, 0), "");
  assert.equal(formatResetRemaining(100, 100_000), "곧 초기화");
  assert.equal(formatResetRemaining(130, 100_000), "1분 남음");
  assert.equal(formatResetRemaining(7300, 100_000), "2시간 남음");
  assert.equal(formatResetRemaining(172900, 100_000), "2일 남음");
});

test("usage warning colors change at 70 and 90 percent", () => {
  assert.equal(severityClass(69.9), "");
  assert.equal(severityClass(70), "warn");
  assert.equal(severityClass(89.9), "warn");
  assert.equal(severityClass(90), "hot");
});

test("update time comes from the completed fetch, not the current render time", () => {
  assert.equal(formatUpdatedAt(null), "");
  const fetchedAt = 1700000000;
  assert.equal(formatUpdatedAt(fetchedAt), new Date(fetchedAt * 1000).toLocaleTimeString([], {
    hour: "2-digit", minute: "2-digit", hour12: false,
  }));
});

test("credit balances and estimates keep their values instead of becoming zero percent", () => {
  assert.equal(formatTrackCaption({ percent: null, value: "125.5 크레딧 남음", resetsAt: null }, 0), "125.5 크레딧 남음");
  assert.equal(formatTrackCaption({ percent: null, value: "약 5–20회", resetsAt: null }, 0), "약 5–20회");
  assert.equal(formatTrackCaption({ percent: 75, value: "750 / 1000 크레딧 사용", resetsAt: 7300 }, 100_000), "75% · 2시간 남음");
});
