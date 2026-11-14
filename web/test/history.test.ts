import { describe, expect, it } from "vitest";
import { dropLastTurn, lastQuestion, latestContext, toHistory, type HistoryItem } from "../src/features/chat/history";

const ITEMS: HistoryItem[] = [
  { role: "user", text: "첫 질문" },
  { role: "assistant", text: "첫 답" },
  { role: "user", text: "둘째 질문" },
  { role: "verdict", text: "판정" },
  { role: "upload", text: "[업로드] a.pdf, 3쪽" },
  { role: "assistant", text: "둘째 답" },
];

describe("history turns", () => {
  it("finds the last question", () => {
    expect(lastQuestion(ITEMS)).toBe("둘째 질문");
    expect(lastQuestion([{ role: "upload", text: "x" }])).toBeNull();
  });

  it("drops the last turn but keeps upload lines that followed it", () => {
    expect(dropLastTurn(ITEMS)).toEqual([
      { role: "user", text: "첫 질문" },
      { role: "assistant", text: "첫 답" },
      { role: "upload", text: "[업로드] a.pdf, 3쪽" },
    ]);
    const noUser: HistoryItem[] = [{ role: "upload", text: "x" }];
    expect(dropLastTurn(noUser)).toEqual(noUser);
  });
});

describe("context in history", () => {
  it("parses the saved context of an answer", () => {
    const items = toHistory({
      messages: [
        { role: "user", text: "q" },
        { role: "assistant", text: "a", context: { used: 22204, limit: 32768, summary_at: 20000 } },
      ],
    });
    expect(items[1]?.context).toEqual({ used: 22204, limit: 32768, summaryAt: 20000 });
    expect(items[0]?.context).toBeUndefined();
  });

  it("prefers the latest live turn, then the latest history item", () => {
    const history = toHistory({ messages: [{ role: "assistant", text: "a", context: { used: 100, limit: 32768, summary_at: 20000 } }] });
    expect(latestContext([], history)?.used).toBe(100);
    expect(latestContext([{ context: null }, { context: { used: 200, limit: 32768, summaryAt: 20000 } }], history)?.used).toBe(200);
    expect(latestContext([{ context: null }], [])).toBeNull();
  });
});
