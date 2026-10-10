// Bounded in-memory cache for opaque, deterministic derived responses.
// It never stores source models, STEP bytes, errors, provider output, or CAD
// objects, and it is intentionally lost when the Node process exits.

export const ArtifactKind = Object.freeze({
  PrincipalDrawing: "principal_drawing",
  ThreeViewDrawing: "three_view_drawing",
});

const CACHEABLE = new Set(Object.values(ArtifactKind));

export class DerivedArtifactCache {
  constructor(maxEntries = 8) {
    if (!Number.isInteger(maxEntries) || maxEntries < 1) throw new TypeError("maxEntries must be a positive integer");
    this.maxEntries = maxEntries;
    this.entries = new Map();
  }

  get(modelFingerprint, artifactKind) {
    const key = cacheKey(modelFingerprint, artifactKind);
    const value = this.entries.get(key);
    if (value === undefined) return undefined;
    this.entries.delete(key);
    this.entries.set(key, value);
    return structuredClone(value);
  }

  set(modelFingerprint, artifactKind, value) {
    const key = cacheKey(modelFingerprint, artifactKind);
    this.entries.delete(key);
    this.entries.set(key, structuredClone(value));
    while (this.entries.size > this.maxEntries) this.entries.delete(this.entries.keys().next().value);
  }

  clear() { this.entries.clear(); }
  get size() { return this.entries.size; }
  has(modelFingerprint, artifactKind) { return this.entries.has(cacheKey(modelFingerprint, artifactKind)); }
}

function cacheKey(modelFingerprint, artifactKind) {
  if (typeof modelFingerprint !== "string" || !/^[0-9a-f]{64}$/.test(modelFingerprint)) throw new TypeError("cache key requires a server fingerprint");
  if (!CACHEABLE.has(artifactKind)) throw new TypeError("artifact kind is not cacheable");
  return `${modelFingerprint}:${artifactKind}`;
}
