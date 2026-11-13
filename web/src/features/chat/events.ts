import type { SseMessage } from "../../shared/api/sse";

export const STREAM_ERROR_TEXT = "답을 만들지 못했습니다. 잠시 뒤 다시 시도해 주세요";

export type Source = {
  kind: "law" | "case";
  label: string;
  note: string;
  displays: string[];
  body: string;
  spans: [number, number][];
};

export type ChatEvent =
  | { kind: "status"; text: string }
  | { kind: "token"; text: string }
  | { kind: "verdict"; text: string }
  | { kind: "needed"; asks: string[] }
  | { kind: "warning"; message: string }
  | { kind: "sources"; items: Source[] }
  | { kind: "resume"; question: string }
  | { kind: "error"; message: string; requestId: string }
  | { kind: "done" };

function parse(data: string): Record<string, unknown> | null {
  try {
    const value: unknown = JSON.parse(data);
    return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string" && v !== "") : [];
}

function spans(value: unknown, length: number): [number, number][] {
  if (!Array.isArray(value)) {
    return [];
  }
  const out: [number, number][] = [];
  for (const pair of value) {
    if (Array.isArray(pair) && typeof pair[0] === "number" && typeof pair[1] === "number" && pair[0] < pair[1] && pair[1] <= length) {
      out.push([pair[0], pair[1]]);
    }
  }
  return out;
}

export function toSource(value: unknown): Source | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  const v = value as Record<string, unknown>;
  const kind = v.kind === "law" || v.kind === "case" ? v.kind : null;
  const label = typeof v.label === "string" ? v.label : "";
  const body = typeof v.body === "string" ? v.body : "";
  if (!kind || label === "" || body === "") {
    return null;
  }
  return {
    kind,
    label,
    note: typeof v.note === "string" ? v.note : "",
    displays: strings(v.displays),
    body,
    spans: spans(v.spans, [...body].length),
  };
}

function text(value: unknown): string | null {
  return typeof value === "string" && value !== "" ? value : null;
}

export function toChatEvent(message: SseMessage): ChatEvent | null {
  const data = parse(message.data);
  switch (message.event) {
    case "Status": {
      const t = text(data?.text);
      return t ? { kind: "status", text: t } : null;
    }
    case "Token": {
      const t = text(data?.text);
      return t ? { kind: "token", text: t } : null;
    }
    case "VerdictReady": {
      const t = text(data?.text);
      return t ? { kind: "verdict", text: t } : null;
    }
    case "NeededFacts": {
      const raw = data?.asks;
      const asks = Array.isArray(raw) ? raw.filter((a): a is string => typeof a === "string" && a !== "") : [];
      return asks.length > 0 ? { kind: "needed", asks } : null;
    }
    case "Warning": {
      const m = text(data?.message);
      return m ? { kind: "warning", message: m } : null;
    }
    case "Resume": {
      const q = text(data?.question);
      return q ? { kind: "resume", question: q } : null;
    }
    case "Sources": {
      const raw = data?.items;
      const items = Array.isArray(raw) ? raw.map(toSource).filter((s): s is Source => s !== null) : [];
      return items.length > 0 ? { kind: "sources", items } : null;
    }
    case "Error":
      return { kind: "error", message: text(data?.message) ?? STREAM_ERROR_TEXT, requestId: text(data?.request_id) ?? "" };
    case "Done":
      return { kind: "done" };
    default:
      return null;
  }
}
