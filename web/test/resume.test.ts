import { describe, expect, it } from "vitest";
import { toChatEvent } from "../src/features/chat/events";
import { dropPending } from "../src/features/chat/history";

describe("resuming a running answer", () => {
  it("reads the Resume event", () => {
    expect(toChatEvent({ event: "Resume", data: JSON.stringify({ question: "요건은?" }) } as never)).toEqual({ kind: "resume", question: "요건은?" });
    expect(toChatEvent({ event: "Resume", data: "{}" } as never)).toBeNull();
  });

  it("drops only a trailing question that the live turn will show", () => {
    const items = [
      { role: "user" as const, text: "앞 질문" },
      { role: "assistant" as const, text: "앞 답" },
      { role: "user" as const, text: "요건은?" },
    ];
    expect(dropPending(items, "요건은?")).toEqual(items.slice(0, 2));
    expect(dropPending(items.slice(0, 2), "요건은?")).toEqual(items.slice(0, 2));
  });
});
