import { toContextUse, toSource, type ContextUse, type Source } from "./events";

export type HistoryRole = "user" | "assistant" | "verdict" | "upload";

export type HistoryItem = {
  role: HistoryRole;
  text: string;
  sources?: Source[];
  context?: ContextUse;
};

const ROLES: ReadonlySet<string> = new Set<HistoryRole>(["user", "assistant", "verdict", "upload"]);

export function toHistory(body: unknown): HistoryItem[] {
  const raw = body && typeof body === "object" && "messages" in body ? body.messages : null;
  if (!Array.isArray(raw)) {
    return [];
  }
  const out: HistoryItem[] = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") {
      continue;
    }
    const role: unknown = "role" in item ? item.role : null;
    const text: unknown = "text" in item ? item.text : null;
    if (typeof role === "string" && ROLES.has(role) && typeof text === "string") {
      const raw: unknown = "sources" in item ? item.sources : null;
      const sources = Array.isArray(raw) ? raw.map(toSource).filter((s): s is Source => s !== null) : [];
      const entry: HistoryItem = { role: role as HistoryRole, text };
      if (sources.length > 0) {
        entry.sources = sources;
      }
      const context = toContextUse("context" in item ? item.context : null);
      if (context) {
        entry.context = context;
      }
      out.push(entry);
    }
  }
  return out;
}

export function dropPending(items: HistoryItem[], question: string): HistoryItem[] {
  const last = items[items.length - 1];
  return last && last.role === "user" && last.text.trim() === question.trim() ? items.slice(0, -1) : items;
}

export function lastQuestion(items: HistoryItem[]): string | null {
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    if (item?.role === "user") {
      return item.text;
    }
  }
  return null;
}

export function dropLastTurn(items: HistoryItem[]): HistoryItem[] {
  for (let i = items.length - 1; i >= 0; i--) {
    if (items[i]?.role === "user") {
      return [...items.slice(0, i), ...items.slice(i + 1).filter((item) => item.role === "upload")];
    }
  }
  return items;
}

export function latestContext(turns: readonly { context: ContextUse | null }[], items: HistoryItem[]): ContextUse | null {
  for (let i = turns.length - 1; i >= 0; i--) {
    const c = turns[i]?.context;
    if (c) {
      return c;
    }
  }
  for (let i = items.length - 1; i >= 0; i--) {
    const c = items[i]?.context;
    if (c) {
      return c;
    }
  }
  return null;
}
