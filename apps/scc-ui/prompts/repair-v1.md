The previous output was invalid and was rejected by the engineering validator.
Fix ONLY the reported error. Return the corrected output in the same contract as before.

Rules:
1. Never change confirmed user values. Explicit dimensions the user provided must stay exactly as given.
2. Correct only the reported structural problem (missing field, unknown tool, wrong argument, type error, etc.).
3. Do not expand patterns and do not regenerate the whole design.
4. If you cannot fix the output without guessing a critical value, return a clarification instead.
