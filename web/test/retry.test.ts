import { describe, expect, it } from "vitest";
import { retry } from "../src/features/chat/api";
import type { ChatEvent } from "../src/features/chat/events";
import type { HistoryItem } from "../src/features/chat/history";
import { startTurn, type Turn } from "../src/features/chat/turn";
import { retryTarget } from "../src/features/chat/useConversation";
import { ApiClient } from "../src/shared/api/client";
import { memoryStore } from "../src/shared/api/tokenStore";
import { fakeFetch, stream } from "./fakeFetch";

const sse = (chunks: string[]) => () => new Response(stream(chunks), { status: 200, headers: { "content-type": "text/event-stream" } });

describe("retry request", () => {
  it("posts to the retry path without a body and streams the new answer", async () => {
    const f = fakeFetch(sse(['event: Token\ndata: {"text":"다시"}\n\nevent: Done\ndata: {}\n\n']));
    const client = new ApiClient({ tokens: memoryStore("tok"), fetch: f.fn });
    const events: ChatEvent[] = [];
    expect(await retry(client, "s-1", (e) => events.push(e))).toBe("done");
    expect(events).toEqual([{ kind: "token", text: "다시" }, { kind: "done" }]);
    expect(f.calls[0]?.url).toBe("/session/s-1/retry");
    expect(f.calls[0]?.method).toBe("POST");
    expect(f.calls[0]?.body).toBeUndefined();
    expect(f.calls[0]?.headers.get("x-csrf-token")).toBe("tok");
  });
});

describe("retry target", () => {
  const history: HistoryItem[] = [
    { role: "user", text: "앞 질문" },
    { role: "assistant", text: "앞 답" },
  ];

  it("uses the last live turn when there is one, unless it is still streaming", () => {
    const done: Turn = { ...startTurn(1, "지금 질문"), state: "done" };
    expect(retryTarget([done], history)).toEqual({ question: "지금 질문", resend: false });
    expect(retryTarget([startTurn(2, "진행 중")], history)).toBeNull();
  });

  it("resends the question only when the server never received it", () => {
    const refused: Turn = { ...startTurn(1, "거절된 질문"), state: "error", error: { message: "바쁨", requestId: "" } };
    const failed: Turn = { ...startTurn(2, "실패한 질문"), state: "error", error: { message: "실패", requestId: "r1" } };
    const broken: Turn = { ...startTurn(3, "끊긴 질문"), state: "broken", error: { message: "끊김", requestId: "" } };
    expect(retryTarget([refused], history)?.resend).toBe(true);
    expect(retryTarget([failed], history)?.resend).toBe(false);
    expect(retryTarget([broken], history)?.resend).toBe(false);
  });

  it("falls back to the last question in the loaded history", () => {
    expect(retryTarget([], history)).toEqual({ question: "앞 질문", resend: false });
    expect(retryTarget([], [{ role: "upload", text: "x" }])).toBeNull();
  });
});
