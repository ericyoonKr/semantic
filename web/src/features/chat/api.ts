import type { ApiClient } from "../../shared/api/client";
import { ApiError, FALLBACK_TEXT } from "../../shared/api/errors";
import { readSse } from "../../shared/api/sse";
import { toChatEvent, type ChatEvent } from "./events";
import { toHistory, type HistoryItem } from "./history";

export type SessionSummary = {
  id: string;
  title: string | null;
};

export type AskEnd = "done" | "error" | "broken";

export async function listSessions(client: ApiClient): Promise<SessionSummary[]> {
  const body = await client.json<unknown>("GET", "/api/sessions");
  if (!Array.isArray(body)) {
    return [];
  }
  return body.flatMap((row: unknown) => {
    if (!row || typeof row !== "object" || !("session_id" in row) || typeof row.session_id !== "string") {
      return [];
    }
    const title = "title" in row && typeof row.title === "string" && row.title.trim() !== "" ? row.title : null;
    return [{ id: row.session_id, title }];
  });
}

export async function createSession(client: ApiClient): Promise<string> {
  const body = await client.json<{ session_id?: unknown }>("POST", "/session");
  if (typeof body.session_id !== "string") {
    throw new ApiError(500, FALLBACK_TEXT);
  }
  return body.session_id;
}

export async function deleteSession(client: ApiClient, id: string): Promise<void> {
  await client.empty("DELETE", `/session/${encodeURIComponent(id)}`);
}

export const TITLE_MAX_CHARS = 80;

export async function renameSession(client: ApiClient, id: string, title: string): Promise<void> {
  await client.empty("PATCH", `/session/${encodeURIComponent(id)}`, { json: { title } });
}

export async function loadHistory(client: ApiClient, id: string): Promise<HistoryItem[]> {
  return toHistory(await client.json<unknown>("GET", `/session/${encodeURIComponent(id)}/history`));
}

export async function ask(client: ApiClient, id: string, question: string, onEvent: (event: ChatEvent) => void, signal?: AbortSignal): Promise<AskEnd> {
  const options = signal ? { json: { question }, accept: "text/event-stream", signal } : { json: { question }, accept: "text/event-stream" };
  const res = await client.send("POST", `/session/${encodeURIComponent(id)}/ask`, options);
  return readEvents(res, onEvent);
}

export async function attachLive(client: ApiClient, id: string, onEvent: (event: ChatEvent) => void, signal?: AbortSignal): Promise<AskEnd | "none"> {
  const options = signal ? { accept: "text/event-stream", signal } : { accept: "text/event-stream" };
  const res = await client.send("GET", `/session/${encodeURIComponent(id)}/live`, options);
  if (res.status === 204) {
    return "none";
  }
  return readEvents(res, onEvent);
}

async function readEvents(res: Response, onEvent: (event: ChatEvent) => void): Promise<AskEnd> {
  if (!res.body) {
    return "broken";
  }
  let end: AskEnd = "broken";
  try {
    await readSse(res.body, (message) => {
      const event = toChatEvent(message);
      if (!event) {
        return true;
      }
      onEvent(event);
      if (event.kind === "done" || event.kind === "error") {
        end = event.kind;
        return false;
      }
      return true;
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw error;
    }
    return "broken";
  }
  return end;
}
