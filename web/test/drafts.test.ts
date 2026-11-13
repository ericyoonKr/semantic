import { describe, expect, it } from "vitest";
import { readDraft, writeDraft } from "../src/features/chat/drafts";

describe("질문 초안", () => {
  it("대화마다 쓰던 글을 따로 기억하고 보내면 지운다", () => {
    writeDraft("s1", "지방공무원법 해당");
    writeDraft("s2", "과징금은");
    expect(readDraft("s1")).toBe("지방공무원법 해당");
    expect(readDraft("s2")).toBe("과징금은");
    writeDraft("s1", "");
    expect(readDraft("s1")).toBe("");
    expect(readDraft("s3")).toBe("");
  });
});
