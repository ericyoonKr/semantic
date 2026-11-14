import { describe, expect, it } from "vitest";
import { contextLabel } from "../src/features/chat/ContextBar";

describe("context bar", () => {
  it("labels the used and total tokens with thousands separators", () => {
    expect(contextLabel({ used: 22204, limit: 32768, summaryAt: 20000 })).toBe("문맥 22,204 / 32,768");
  });
});

describe("context percent", () => {
  it("rounds the share of the limit and caps it at 100", async () => {
    const { contextPercent } = await import("../src/features/chat/ContextBar");
    expect(contextPercent({ used: 12994, limit: 32768, summaryAt: 20000 })).toBe(40);
    expect(contextPercent({ used: 40000, limit: 32768, summaryAt: 20000 })).toBe(100);
  });
});
