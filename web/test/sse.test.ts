import { describe, expect, it } from "vitest";
import { SseParser, readSse, type SseMessage } from "../src/shared/api/sse";

const CR = String.fromCharCode(13);
const LF = String.fromCharCode(10);

function parseAll(chunks: string[]): SseMessage[] {
  const p = new SseParser();
  return [...chunks.flatMap((c) => p.push(c)), ...p.end()];
}

function bytesStream(parts: Uint8Array[]): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(controller) {
      for (const part of parts) {
        controller.enqueue(part);
      }
      controller.close();
    },
  });
}

const STREAM = [
  'event: Status\ndata: {"text":"자료를 찾는 중"}\n\n',
  'event: Token\ndata: {"text":"첫 조각"}\n\n',
  'event: VerdictReady\ndata: {"text":"요건을 충족합니다"}\n\n',
  "event: Done\ndata: {}\n\n",
].join("");

const EXPECTED: SseMessage[] = [
  { event: "Status", data: '{"text":"자료를 찾는 중"}' },
  { event: "Token", data: '{"text":"첫 조각"}' },
  { event: "VerdictReady", data: '{"text":"요건을 충족합니다"}' },
  { event: "Done", data: "{}" },
];

describe("SseParser", () => {
  it("parses a whole stream in one chunk", () => {
    expect(parseAll([STREAM])).toEqual(EXPECTED);
  });

  it("gives the same events for every split point", () => {
    for (let i = 0; i <= STREAM.length; i++) {
      expect(parseAll([STREAM.slice(0, i), STREAM.slice(i)])).toEqual(EXPECTED);
    }
  });

  it("gives the same events when fed one character at a time", () => {
    expect(parseAll([...STREAM])).toEqual(EXPECTED);
  });

  it("accepts CRLF and lone CR line endings at every split point", () => {
    for (const eol of [CR + LF, CR]) {
      const text = STREAM.split(LF).join(eol);
      for (let i = 0; i <= text.length; i++) {
        expect(parseAll([text.slice(0, i), text.slice(i)])).toEqual(EXPECTED);
      }
    }
  });

  it("waits for the next chunk before treating a trailing CR as the line end", () => {
    const p = new SseParser();
    expect(p.push("event: Done" + CR + "data: {}" + CR + CR)).toEqual([]);
    expect(p.push(LF)).toEqual([{ event: "Done", data: "{}" }]);
  });

  it("joins multi-line data with newlines and strips one leading space only", () => {
    expect(parseAll(["event: Token\ndata:  a\ndata:b\n\n"])).toEqual([{ event: "Token", data: " a\nb" }]);
  });

  it("ignores comments, id and retry lines and blocks without data", () => {
    expect(parseAll([": keep-alive\n\nid: 7\nretry: 100\n\nevent: Done\n\n", "event: Done\ndata: {}\n\n"])).toEqual([
      { event: "Done", data: "{}" },
    ]);
  });

  it("uses message as the default event name and a bare data field as empty", () => {
    expect(parseAll(["data\n\n", "data: x\n\n"])).toEqual([
      { event: "message", data: "" },
      { event: "message", data: "x" },
    ]);
  });

  it("does not dispatch an unfinished event", () => {
    expect(parseAll(['event: Token\ndata: {"text":"끝나지 않음"}\n'])).toEqual([]);
  });
});

describe("readSse", () => {
  it("decodes multibyte characters split across byte chunks", async () => {
    const bytes = new TextEncoder().encode(STREAM);
    for (let cut = 1; cut < bytes.length; cut++) {
      const got: SseMessage[] = [];
      await readSse(bytesStream([bytes.slice(0, cut), bytes.slice(cut)]), (m) => {
        got.push(m);
      });
      expect(got).toEqual(EXPECTED);
    }
  });

  it("stops reading and cancels when the handler returns false", async () => {
    let cancelled = false;
    const bytes = new TextEncoder().encode(STREAM);
    const body = new ReadableStream<Uint8Array>({
      pull(controller) {
        controller.enqueue(bytes);
      },
      cancel() {
        cancelled = true;
      },
    });
    const got: SseMessage[] = [];
    await readSse(body, (m) => {
      got.push(m);
      return m.event !== "Token";
    });
    expect(got.map((m) => m.event)).toEqual(["Status", "Token"]);
    expect(cancelled).toBe(true);
  });
});
