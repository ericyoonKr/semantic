import type { ApiClient } from "../../shared/api/client";

export type OutlineEntry = {
  level: number;
  title: string;
  page: number | null;
};

export type UploadSummary = {
  fileId: string;
  filename: string;
  format: string;
  pages: number;
  outline: OutlineEntry[];
  warnings: string[];
  findingsCount: number;
};

function num(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

export function toSummary(body: unknown): UploadSummary {
  const row = body && typeof body === "object" ? (body as Record<string, unknown>) : {};
  const outline = Array.isArray(row.outline)
    ? row.outline.flatMap((entry: unknown) => {
        if (!entry || typeof entry !== "object") {
          return [];
        }
        const e = entry as Record<string, unknown>;
        const title = str(e.title);
        return title ? [{ level: num(e.level, 1), title, page: typeof e.page === "number" ? e.page : null }] : [];
      })
    : [];
  const warnings = Array.isArray(row.warnings) ? row.warnings.filter((w): w is string => typeof w === "string" && w !== "") : [];
  return {
    fileId: str(row.file_id),
    filename: str(row.filename),
    format: str(row.format),
    pages: num(row.pages, 0),
    outline,
    warnings,
    findingsCount: num(row.findings_count, 0),
  };
}

export async function uploadFile(client: ApiClient, sessionId: string, file: File): Promise<UploadSummary> {
  const form = new FormData();
  form.append("file", file, file.name);
  return toSummary(await client.json<unknown>("POST", `/session/${encodeURIComponent(sessionId)}/upload`, { form }));
}

export const UPLOAD_PREFIX = "[업로드] ";

export function uploadLabel(text: string): string {
  return text.startsWith(UPLOAD_PREFIX) ? text.slice(UPLOAD_PREFIX.length) : text;
}
