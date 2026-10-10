import assert from "node:assert/strict";
import test from "node:test";
import { createApp } from "../server.mjs";

async function withServer(run) {
  const server = createApp({ adapter: { version: async () => "scc test" } });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try { await run(`http://127.0.0.1:${server.address().port}`); }
  finally { await new Promise((resolve) => server.close(resolve)); }
}

test("canonical Engineering page uses only same-origin external scripts and styles under CSP", async () => {
  await withServer(async (base) => {
    const root = await fetch(base, { redirect: "manual" });
    assert.equal(root.status, 302);
    assert.equal(root.headers.get("location"), "/engineering.html");
    const response = await fetch(`${base}/engineering.html`);
    assert.equal(response.status, 200);
    const html = await response.text();
    const csp = response.headers.get("content-security-policy") ?? "";
    assert.match(csp, /script-src 'self'/);
    assert.match(csp, /style-src 'self'/);
    assert.match(html, /src="\/js\/engineering-page.js"/);
    assert.match(html, /href="\/css\/engineering.css"/);
    assert.doesNotMatch(html, /<script(?![^>]*src=)[^>]*>/);
    assert.doesNotMatch(html, /<style[^>]*>/);
    assert.equal((await fetch(`${base}/js/engineering-page.js`)).status, 200);
    assert.equal((await fetch(`${base}/css/engineering.css`)).status, 200);
  });
});
