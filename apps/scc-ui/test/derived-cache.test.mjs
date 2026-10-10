import assert from "node:assert/strict";
import test from "node:test";
import { ArtifactKind, DerivedArtifactCache } from "../lib/derived-cache.mjs";

const A = "a".repeat(64);
const B = "b".repeat(64);

test("derived cache separates artifact kinds and returns defensive copies", () => {
  const cache = new DerivedArtifactCache(4);
  const principal = { svg: "principal", model_fingerprint: A };
  cache.set(A, ArtifactKind.PrincipalDrawing, principal);
  cache.set(A, ArtifactKind.ThreeViewDrawing, { svg: "three" });
  const copy = cache.get(A, ArtifactKind.PrincipalDrawing);
  copy.svg = "tampered";
  assert.equal(cache.get(A, ArtifactKind.PrincipalDrawing).svg, "principal");
  assert.equal(cache.get(A, ArtifactKind.ThreeViewDrawing).svg, "three");
  assert.equal(cache.get(B, ArtifactKind.PrincipalDrawing), undefined);
});

test("derived cache is bounded and clearing it cannot change the source key", () => {
  const cache = new DerivedArtifactCache(2);
  cache.set(A, ArtifactKind.PrincipalDrawing, { svg: "a" });
  cache.set(A, ArtifactKind.ThreeViewDrawing, { svg: "a3" });
  cache.get(A, ArtifactKind.PrincipalDrawing);
  cache.set(B, ArtifactKind.PrincipalDrawing, { svg: "b" });
  assert.equal(cache.size, 2);
  assert.equal(cache.get(A, ArtifactKind.ThreeViewDrawing), undefined);
  assert.equal(cache.get(A, ArtifactKind.PrincipalDrawing).svg, "a");
  cache.clear();
  assert.equal(cache.size, 0);
  assert.equal(cache.get(A, ArtifactKind.PrincipalDrawing), undefined);
});

test("derived cache rejects non-server fingerprints and non-cacheable artifacts", () => {
  const cache = new DerivedArtifactCache();
  assert.throws(() => cache.set("client-value", ArtifactKind.PrincipalDrawing, {}), /server fingerprint/);
  assert.throws(() => cache.set(A, "step", {}), /not cacheable/);
});
