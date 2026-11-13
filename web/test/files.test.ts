import { describe, expect, it } from "vitest";
import { originalUrl, showsOriginal, toFileText, toPages, toStoredFiles } from "../src/features/upload/files";

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
    expect(toStoredFiles({ files: [{ file_id: "f1", filename: "a.pdf", pages: 2, format: "PDF" }, { filename: "x" }] })).toEqual([{ fileId: "f1", filename: "a.pdf", pages: 2, format: "pdf" }]);
    expect(toFileText({ file_id: "f1", filename: "a.pdf", pages: 2, text: "글" })?.text).toBe("글");
    expect(toFileText({ file_id: "f1" })).toBeNull();
  });

  it("shows only PDFs as the original and builds a same-origin original path", () => {
    const base = { fileId: "f 1", filename: "a", pages: 1 };
    expect(showsOriginal({ ...base, format: "pdf" })).toBe(true);
    expect(showsOriginal({ ...base, format: "docx" })).toBe(false);
    expect(showsOriginal({ ...base, format: "hwpx" })).toBe(false);
    expect(originalUrl("s/1", "f 1")).toBe("/session/s%2F1/files/f%201/original");
  });
});
