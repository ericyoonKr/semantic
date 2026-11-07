import { describe, expect, it } from "vitest";
import { ask } from "../src/features/chat/api";
import type { ChatEvent } from "../src/features/chat/events";
import { ApiClient } from "../src/shared/api/client";
import { ApiError } from "../src/shared/api/errors";
import { memoryStore } from "../src/shared/api/tokenStore";
import { fakeFetch, json, stream } from "./fakeFetch";

function run(res: () => Response) {
  const f = fakeFetch(res);
  const client = new ApiClient({ tokens: memoryStore("tok"), fetch: f.fn });
  const events: ChatEvent[] = [];
  const done = ask(client, "s-1", "질문", (e) => events.push(e));
  return { done, events, calls: f.calls };
}

const sse = (chunks: string[], fail = false) => () => new Response(stream(chunks, fail), { status: 200, headers: { "content-type": "text/event-stream" } });

describe("ask", () => {
  it("posts the question with the CSRF token and streams events as they arrive", async () => {
    const r = run(
      sse([
        'event: Status\ndata: {"text":"자료를 찾는 중"}\n\nevent: To',
        'ken\ndata: {"text":"앞"}\n',
        '\nevent: Token\ndata: {"text":"뒤"}\n\nevent: Done\ndata: {}\n\n',
      ]),
    );
    expect(await r.done).toBe("done");
    expect(r.events).toEqual([
      { kind: "status", text: "자료를 찾는 중" },
      { kind: "token", text: "앞" },
      { kind: "token", text: "뒤" },
      { kind: "done" },
    ]);
    expect(r.calls[0]?.url).toBe("/session/s-1/ask");
    expect(r.calls[0]?.headers.get("x-csrf-token")).toBe("tok");
    expect(JSON.parse(String(r.calls[0]?.body))).toEqual({ question: "질문" });
  });

  it("ends with error after an Error frame and keeps the request id", async () => {
    const r = run(sse(['event: Error\ndata: {"message":"답을 만들지 못했습니다","request_id":"r9"}\n\n', 'event: Token\ndata: {"text":"x"}\n\n']));
    expect(await r.done).toBe("error");
    expect(r.events).toEqual([{ kind: "error", message: "답을 만들지 못했습니다", requestId: "r9" }]);
  });

  it("reports a broken stream when it ends without Done", async () => {
    const r = run(sse(['event: Token\ndata: {"text":"앞"}\n\nevent: Token\ndata: {"text":"반쪽']));
    expect(await r.done).toBe("broken");
    expect(r.events).toEqual([{ kind: "token", text: "앞" }]);
  });

  it("reports a broken stream when the connection fails midway", async () => {
    const r = run(sse(['event: Token\ndata: {"text":"앞"}\n\n'], true));
    expect(await r.done).toBe("broken");
  });

  it("throws the gateway message when the ask is refused", async () => {
    const r = run(() => json(409, { message: "이 대화에서 답을 쓰는 중입니다" }));
    const e = await r.done.catch((x: unknown) => x);
    expect(e).toBeInstanceOf(ApiError);
    expect((e as ApiError).message).toBe("이 대화에서 답을 쓰는 중입니다");
    expect(r.events).toEqual([]);
  });
});
