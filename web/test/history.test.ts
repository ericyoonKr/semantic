import { describe, expect, it } from "vitest";
import { dropLastTurn, lastQuestion, type HistoryItem } from "../src/features/chat/history";

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
