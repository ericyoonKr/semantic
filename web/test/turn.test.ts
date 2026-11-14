import { describe, expect, it } from "vitest";
import { toHistory } from "../src/features/chat/history";
import { BROKEN_TEXT, applyEvent, breakTurn, startTurn } from "../src/features/chat/turn";
import { toSummary, uploadLabel } from "../src/features/upload/api";

describe("turn", () => {
  it("collects status, answer, verdicts, asks and warnings", () => {
    let t = startTurn(1, "q");
    t = applyEvent(t, { kind: "status", text: "자료를 찾는 중" });
    t = applyEvent(t, { kind: "warning", message: "일부 자료를 찾지 못했습니다" });
    t = applyEvent(t, { kind: "warning", message: "일부 자료를 찾지 못했습니다" });
    t = applyEvent(t, { kind: "verdict", text: "판정" });
    t = applyEvent(t, { kind: "needed", asks: ["a", "b"] });
    t = applyEvent(t, { kind: "needed", asks: ["b", "c"] });
    t = applyEvent(t, { kind: "token", text: "앞" });
    t = applyEvent(t, { kind: "token", text: "뒤" });
    expect(t.status).toBe("자료를 찾는 중");
    t = applyEvent(t, { kind: "done" });
    expect(t).toMatchObject({ state: "done", status: null, answer: "앞뒤", verdicts: ["판정"], asks: ["a", "b", "c"], warnings: ["일부 자료를 찾지 못했습니다"] });
  });

  it("ignores events after the turn ended", () => {
    const t = applyEvent(startTurn(1, "q"), { kind: "error", message: "m", requestId: "r" });
    expect(applyEvent(t, { kind: "token", text: "x" })).toBe(t);
    expect(breakTurn(t)).toBe(t);
    expect(t.error).toEqual({ message: "m", requestId: "r" });
  });

  it("marks a broken stream", () => {
    const t = breakTurn(applyEvent(startTurn(1, "q"), { kind: "token", text: "x" }));
    expect(t.state).toBe("broken");
    expect(t.error?.message).toBe(BROKEN_TEXT);
    expect(t.answer).toBe("x");
  });
});

describe("history and upload shapes", () => {
  it("keeps only known roles with text", () => {
    const body = {
      messages: [
        { role: "user", text: "질문" },
        { role: "tool", text: "x" },
        { role: "assistant", text: "답" },
        { role: "verdict" },
        { role: "upload", text: "[업로드] a.pdf, 3쪽" },
      ],
    };
    expect(toHistory(body)).toEqual([
      { role: "user", text: "질문" },
      { role: "assistant", text: "답" },
      { role: "upload", text: "[업로드] a.pdf, 3쪽" },
    ]);
    expect(toHistory(null)).toEqual([]);
    expect(toHistory({ messages: "x" })).toEqual([]);
  });

  it("reads the upload summary and tolerates missing fields", () => {
    expect(
      toSummary({
        file_id: "f",
        filename: "a.pdf",
        format: "pdf",
        pages: 3,
        outline: [{ level: 1, title: "제1장", page: 1 }, { title: "" }, "x"],
        warnings: ["표 일부", 3],
        findings_count: 2,
      }),
    ).toEqual({
      fileId: "f",
      filename: "a.pdf",
      format: "pdf",
      pages: 3,
      outline: [{ level: 1, title: "제1장", page: 1 }],
      warnings: ["표 일부"],
      findingsCount: 2,
    });
    expect(toSummary(null)).toEqual({ fileId: "", filename: "", format: "", pages: 0, outline: [], warnings: [], findingsCount: 0 });
    expect(uploadLabel("[업로드] a.pdf, 3쪽")).toBe("a.pdf, 3쪽");
  });
});

describe("turn context", () => {
  it("keeps the latest context usage on the turn", () => {
    const t = applyEvent(startTurn(1, "q"), { kind: "context", used: 22204, limit: 32768, summaryAt: 20000 });
    expect(t.context).toEqual({ used: 22204, limit: 32768, summaryAt: 20000 });
    expect(startTurn(2, "q").context).toBeNull();
  });
});
