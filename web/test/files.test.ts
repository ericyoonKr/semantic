import { describe, expect, it } from "vitest";
import { toFileText, toPages, toStoredFiles } from "../src/features/upload/files";

describe("uploaded file text", () => {
  it("splits the extracted text at page marks", () => {
    const text = ["머리", "[1쪽]", "첫 쪽", "[2쪽]", "둘째", "쪽"].join(String.fromCharCode(10));
    expect(toPages(text)).toEqual([
      { page: null, text: "머리" },
      { page: 1, text: "첫 쪽" },
      { page: 2, text: ["둘째", "쪽"].join(String.fromCharCode(10)) },
    ]);
  });

  it("reads file lists and text, dropping rows without an id", () => {
    expect(toStoredFiles({ files: [{ file_id: "f1", filename: "a.pdf", pages: 2 }, { filename: "x" }] })).toEqual([{ fileId: "f1", filename: "a.pdf", pages: 2 }]);
    expect(toFileText({ file_id: "f1", filename: "a.pdf", pages: 2, text: "글" })?.text).toBe("글");
    expect(toFileText({ file_id: "f1" })).toBeNull();
  });
});
