export type HistoryRole = "user" | "assistant" | "verdict" | "upload";

export type HistoryItem = {
  role: HistoryRole;
  text: string;
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
      out.push({ role: role as HistoryRole, text });
    }
  }
  return out;
}
