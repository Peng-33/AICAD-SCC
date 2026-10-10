import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { extractDocument } from "../lib/documents.mjs";

const testDirectory = resolve(fileURLToPath(new URL(".", import.meta.url)));

test("extracts and normalizes Markdown", async () => {
  const result = await extractDocument({
    name: "brief.md",
    dataBase64: Buffer.from("# Bracket\r\n\r\nCreate a centered hole.\r\n").toString("base64"),
  });
  assert.equal(result.text, "# Bracket\n\nCreate a centered hole.");
  assert.match(result.excerpt, /centered hole/);
});

test("extracts text from the real UI DOCX specification", async () => {
  const path = resolve(testDirectory, "..", "..", "..", "SCC_UI_UX_Design_Specification.docx");
  const bytes = await readFile(path);
  const result = await extractDocument({ name: "spec.docx", dataBase64: bytes.toString("base64") });
  assert.match(result.text, /SCC UI\/UX/);
  assert.match(result.text, /ConstraintIR/);
});

test("rejects unsupported document types", async () => {
  await assert.rejects(
    extractDocument({ name: "script.exe", dataBase64: Buffer.from("nope").toString("base64") }),
    (error) => error.code === "SCC-UI-DOC-002",
  );
});
