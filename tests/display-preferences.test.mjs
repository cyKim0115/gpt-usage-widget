import test from "node:test";
import assert from "node:assert/strict";
import { HIDDEN_TRACKS_KEY, loadHiddenTracks, saveTrackVisibility } from "../.test-dist/display-preferences.js";

function storage(initial = null) {
  const values = new Map(initial === null ? [] : [[HIDDEN_TRACKS_KEY, initial]]);
  return { getItem: (key) => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
}

test("new installs show every server-provided track and damaged settings recover", () => {
  for (const initial of [null, "broken", "null", "{}"])
    assert.deepEqual([...loadHiddenTracks(storage(initial))], []);
  assert.deepEqual([...loadHiddenTracks(storage('[false,"credits:balance",42]'))], ["credits:balance"]);
});

test("individual choices survive reload and a temporarily absent or reordered limit", () => {
  const saved = storage();
  saveTrackVisibility("credits:balance", false, saved);
  saveTrackVisibility("extra-model:secondary", false, saved);
  const kinds = ["extra-model:secondary", "codex:primary", "credits:balance", "code-review:primary"];
  assert.deepEqual(kinds.filter((kind) => !loadHiddenTracks(saved).has(kind)), ["codex:primary", "code-review:primary"]);
  saveTrackVisibility("credits:balance", true, saved);
  assert.deepEqual([...loadHiddenTracks(saved)], ["extra-model:secondary"]);
});

test("saving one option preserves another window's choices and reports storage failure", () => {
  const saved = storage();
  saveTrackVisibility("codex:primary", false, saved);
  saveTrackVisibility("credits:resets", false, saved);
  saveTrackVisibility("codex:primary", true, saved);
  assert.deepEqual([...loadHiddenTracks(saved)], ["credits:resets"]);
  assert.throws(() => saveTrackVisibility("credits:balance", false, {
    getItem: () => null, setItem: () => { throw new Error("storage unavailable"); },
  }), /storage unavailable/);
});
