const PREFIX = "semantic.draft.";
const memory = new Map<string, string>();

export function readDraft(sessionId: string): string {
  const kept = memory.get(sessionId);
  if (kept !== undefined) {
    return kept;
  }
  try {
    return window.localStorage.getItem(PREFIX + sessionId) ?? "";
  } catch {
    return "";
  }
}

export function writeDraft(sessionId: string, text: string): void {
  if (text === "") {
    memory.delete(sessionId);
  } else {
    memory.set(sessionId, text);
  }
  try {
    if (text === "") {
      window.localStorage.removeItem(PREFIX + sessionId);
    } else {
      window.localStorage.setItem(PREFIX + sessionId, text);
    }
  } catch {
    return;
  }
}
