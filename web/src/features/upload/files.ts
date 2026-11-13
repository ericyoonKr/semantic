import type { ApiClient } from "../../shared/api/client";

export type StoredFile = { fileId: string; filename: string; pages: number; format: string };

export type FileText = StoredFile & { text: string };

function toStored(row: unknown): StoredFile | null {
  if (!row || typeof row !== "object") {
    return null;
  }
  const r = row as Record<string, unknown>;
  if (typeof r.file_id !== "string" || r.file_id === "") {
    return null;
  }
  return {
    fileId: r.file_id,
    filename: typeof r.filename === "string" ? r.filename : r.file_id,
    pages: typeof r.pages === "number" ? r.pages : 0,
    format: typeof r.format === "string" ? r.format.toLowerCase() : "",
  };
}

export function toStoredFiles(body: unknown): StoredFile[] {
  const raw = body && typeof body === "object" && "files" in body ? (body as { files: unknown }).files : null;
  return Array.isArray(raw) ? raw.map(toStored).filter((f): f is StoredFile => f !== null) : [];
}

export function toFileText(body: unknown): FileText | null {
  const meta = toStored(body);
  const text = body && typeof body === "object" && "text" in body ? (body as { text: unknown }).text : null;
  return meta && typeof text === "string" ? { ...meta, text } : null;
}

export type Page = { page: number | null; text: string };

const PAGE_MARK = /^\[(\d+)쪽\]\s*$/;

export function toPages(text: string): Page[] {
  const out: Page[] = [];
  let current: Page = { page: null, text: "" };
  for (const line of text.split(String.fromCharCode(10))) {
    const mark = PAGE_MARK.exec(line.trim());
    if (mark?.[1]) {
      if (current.text.trim() !== "" || current.page !== null) {
        out.push(current);
      }
      current = { page: Number(mark[1]), text: "" };
    } else {
      current.text += (current.text === "" ? "" : String.fromCharCode(10)) + line;
    }
  }
  if (current.text.trim() !== "" || current.page !== null) {
    out.push(current);
  }
  return out;
}

export function originalUrl(sessionId: string, fileId: string): string {
  return `/session/${encodeURIComponent(sessionId)}/files/${encodeURIComponent(fileId)}/original`;
}

export function showsOriginal(file: StoredFile): boolean {
  return file.format === "pdf";
}

export async function listFiles(client: ApiClient, sessionId: string): Promise<StoredFile[]> {
  return toStoredFiles(await client.json<unknown>("GET", `/session/${encodeURIComponent(sessionId)}/files`));
}

export async function loadFileText(client: ApiClient, sessionId: string, fileId: string): Promise<FileText | null> {
  return toFileText(await client.json<unknown>("GET", `/session/${encodeURIComponent(sessionId)}/files/${encodeURIComponent(fileId)}`));
}
