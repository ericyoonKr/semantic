import { describe, expect, it } from "vitest";
import { sendsOnKey } from "../src/features/chat/AskForm";

describe("enter to send", () => {
  it("sends on Enter, keeps Shift+Enter and Korean composition for typing", () => {
    expect(sendsOnKey("Enter", false, false)).toBe(true);
    expect(sendsOnKey("Enter", true, false)).toBe(false);
    expect(sendsOnKey("Enter", false, true)).toBe(false);
    expect(sendsOnKey("a", false, false)).toBe(false);
  });
});
