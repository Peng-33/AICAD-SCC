// A deterministic FakeProvider for tests: never touches the network.
//
// It can return fixed DSL text, fixed tool calls, clarification, or throw a
// provider-style error. A `handler` function receives the request and the
// attempt number, so tests can script repairs.

export function createFakeProvider(handler) {
  const provider = {
    name: "fake",
    calls: 0,
    async complete(request) {
      provider.calls += 1;
      return handler(request, provider.calls);
    },
  };
  return provider;
}

/** Scripted responses popped in order (used by most tests). */
export function createScriptedProvider(results) {
  let index = 0;
  return createFakeProvider((_request, call) => {
    if (index >= results.length) {
      throw new Error(`fake provider exhausted after ${results.length} calls`);
    }
    const result = results[index];
    index += 1;
    const value = typeof result === "function" ? result(_request, call) : result;
    return value;
  });
}
