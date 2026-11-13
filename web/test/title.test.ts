import { describe, expect, it } from "vitest";
import { TITLE_MAX_CHARS } from "../src/features/chat/api";
import { cleanTitle } from "../src/features/chat/SessionList";

describe("conversation title", () => {
  it("trims spaces and rejects empty or too long titles", () => {
    expect(cleanTitle("  국가배상   질문 ")).toBe("국가배상 질문");
    expect(cleanTitle("   ")).toBeNull();
    expect(cleanTitle("가".repeat(TITLE_MAX_CHARS + 1))).toBeNull();
    expect(cleanTitle("가".repeat(TITLE_MAX_CHARS))).not.toBeNull();
  });
});
