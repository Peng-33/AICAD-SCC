import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import { promisify } from "node:util";
import { inflateRawSync } from "node:zlib";
import { ServiceError } from "./scc-cli.mjs";

const execFileAsync = promisify(execFile);
const MAX_DOCUMENT_BYTES = 5 * 1024 * 1024;
const TEXT_EXTENSIONS = new Set([".md", ".markdown", ".txt", ".json"]);

export async function extractDocument({ name, dataBase64 }) {
  if (typeof name !== "string" || typeof dataBase64 !== "string") {
    throw new ServiceError(400, "SCC-UI-DOC-001", "Document name and data are required");
  }
  const extension = extname(name).toLowerCase();
  if (![...TEXT_EXTENSIONS, ".docx", ".pdf"].includes(extension)) {
    throw new ServiceError(415, "SCC-UI-DOC-002", "Supported documents are PDF, DOCX, Markdown, text, and JSON");
  }
  const bytes = Buffer.from(dataBase64, "base64");
  if (bytes.length === 0 || bytes.length > MAX_DOCUMENT_BYTES) {
    throw new ServiceError(413, "SCC-UI-DOC-003", "Document must be between 1 byte and 5 MiB");
  }
  let text;
  if (TEXT_EXTENSIONS.has(extension)) text = bytes.toString("utf8");
  if (extension === ".docx") text = extractDocxText(bytes);
  if (extension === ".pdf") text = await extractPdfText(bytes);
  const normalized = text.replace(/\r\n/g, "\n").replace(/\n{3,}/g, "\n\n").trim();
  if (!normalized) {
    throw new ServiceError(422, "SCC-UI-DOC-004", "No readable text was found in this document");
  }
  return { name, bytes: bytes.length, text: normalized, excerpt: normalized.slice(0, 320) };
}

function extractDocxText(bytes) {
  const xml = readZipEntry(bytes, "word/document.xml").toString("utf8");
  return decodeXml(xml
    .replace(/<w:tab\b[^>]*\/>/g, "\t")
    .replace(/<w:br\b[^>]*\/>/g, "\n")
    .replace(/<\/w:(p|tr)>/g, "\n")
    .replace(/<\/w:tc>/g, "\t")
    .replace(/<[^>]+>/g, ""));
}

function readZipEntry(zip, wantedName) {
  const eocd = findSignatureBackward(zip, 0x06054b50);
  if (eocd < 0) throw new ServiceError(422, "SCC-UI-DOC-005", "DOCX archive is malformed");
  const entryCount = zip.readUInt16LE(eocd + 10);
  let cursor = zip.readUInt32LE(eocd + 16);
  for (let index = 0; index < entryCount; index += 1) {
    if (zip.readUInt32LE(cursor) !== 0x02014b50) break;
    const method = zip.readUInt16LE(cursor + 10);
    const compressedSize = zip.readUInt32LE(cursor + 20);
    const nameLength = zip.readUInt16LE(cursor + 28);
    const extraLength = zip.readUInt16LE(cursor + 30);
    const commentLength = zip.readUInt16LE(cursor + 32);
    const localOffset = zip.readUInt32LE(cursor + 42);
    const name = zip.subarray(cursor + 46, cursor + 46 + nameLength).toString("utf8");
    if (name === wantedName) {
      const localNameLength = zip.readUInt16LE(localOffset + 26);
      const localExtraLength = zip.readUInt16LE(localOffset + 28);
      const start = localOffset + 30 + localNameLength + localExtraLength;
      const compressed = zip.subarray(start, start + compressedSize);
      if (method === 0) return compressed;
      if (method === 8) return inflateRawSync(compressed);
      throw new ServiceError(422, "SCC-UI-DOC-006", "DOCX uses an unsupported compression method");
    }
    cursor += 46 + nameLength + extraLength + commentLength;
  }
  throw new ServiceError(422, "SCC-UI-DOC-007", "DOCX is missing word/document.xml");
}

function findSignatureBackward(buffer, signature) {
  for (let index = buffer.length - 22; index >= Math.max(0, buffer.length - 65_557); index -= 1) {
    if (buffer.readUInt32LE(index) === signature) return index;
  }
  return -1;
}

async function extractPdfText(bytes) {
  const directory = await mkdtemp(join(tmpdir(), "scc-ui-pdf-"));
  const input = join(directory, "document.pdf");
  try {
    await writeFile(input, bytes);
    const { stdout } = await execFileAsync("pdftotext", ["-layout", input, "-"], {
      timeout: 15_000,
      maxBuffer: 8 * 1024 * 1024,
      windowsHide: true,
      shell: false,
    });
    return stdout;
  } catch (error) {
    throw new ServiceError(503, "SCC-UI-DOC-008", "PDF extraction requires the pdftotext utility", { cause: error.message });
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

function decodeXml(value) {
  return value
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, "\"")
    .replace(/&apos;/g, "'")
    .replace(/&amp;/g, "&");
}
