import type { SseMessage } from "../../shared/api/sse";

export const STREAM_ERROR_TEXT = "답을 만들지 못했습니다. 잠시 뒤 다시 시도해 주세요";

export type ChatEvent =
  | { kind: "status"; text: string }
  | { kind: "token"; text: string }
  | { kind: "verdict"; text: string }
  | { kind: "needed"; asks: string[] }
  | { kind: "warning"; message: string }
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
    case "Error":
      return { kind: "error", message: text(data?.message) ?? STREAM_ERROR_TEXT, requestId: text(data?.request_id) ?? "" };
    case "Done":
      return { kind: "done" };
    default:
      return null;
  }
}
