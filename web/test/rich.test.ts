import { describe, expect, it } from "vitest";
import { citeNames, markSpans, toBlocks, toSegments } from "../src/features/chat/rich";
import { toChatEvent } from "../src/features/chat/events";

describe("rich answer text", () => {
  it("turns star lines into a list and keeps other lines as paragraphs", () => {
    const blocks = toBlocks("머리말\n*   **목적**: 가\n* 나\n\n끝");
    expect(blocks).toEqual([
      { kind: "para", text: "머리말" },
      { kind: "list", items: ["**목적**: 가", "나"] },
      { kind: "para", text: "끝" },
    ]);
  });

  it("reads heading and rule lines", () => {
    expect(toBlocks(["### 1. 일반적인 허용 여부 (행정 및 계약의 원칙)", "---", "본문", "#해시만"].join("\n"))).toEqual([
      { kind: "heading", level: 3, text: "1. 일반적인 허용 여부 (행정 및 계약의 원칙)" },
      { kind: "rule" },
      { kind: "para", text: "본문" },
      { kind: "para", text: "#해시만" },
    ]);
  });

  it("groups table rows across blank lines and takes the header from the separator", () => {
    const text = ["요약하자면", "", "| 구분 | 법적 쟁점 |", "", "| :--- | :--- |", "", "| 저작권 측면 | **이용 허락** 초과 |", "", "| 계약 측면 | 채무불이행 |", "", "결론적으로"].join(String.fromCharCode(10));
    expect(toBlocks(text)).toEqual([
      { kind: "para", text: "요약하자면" },
      { kind: "table", header: ["구분", "법적 쟁점"], rows: [["저작권 측면", "**이용 허락** 초과"], ["계약 측면", "채무불이행"]] },
      { kind: "para", text: "결론적으로" },
    ]);
  });

  it("keeps an escaped bar inside a table cell", () => {
    const text = ["| 항목 | 값 |", "| --- | --- |", "| A" + String.fromCharCode(92) + "|B | 1 |"].join(String.fromCharCode(10));
    expect(toBlocks(text)).toEqual([{ kind: "table", header: ["항목", "값"], rows: [["A|B", "1"]] }]);
  });

  it("marks bold pairs and leaves an unmatched star pair as text", () => {
    expect(toSegments("**목적**: 정함", [])).toEqual([
      { text: "목적", bold: true, source: null },
      { text: ": 정함", bold: false, source: null },
    ]);
    expect(toSegments("a ** b", [])).toEqual([{ text: "a ** b", bold: false, source: null }]);
  });

  it("links the longest matching citation name to its source", () => {
    const names = citeNames([["정부조직법 제1조(시행 2021-12-30)", "정부조직법 제1조"], ["정부조직법 제10조"]]);
    const segs = toSegments("정한다 정부조직법 제1조(시행 2021-12-30). 또 정부조직법 제10조", names);
    expect(segs.filter((s) => s.source !== null).map((s) => [s.text, s.source])).toEqual([
      ["정부조직법 제1조(시행 2021-12-30)", 0],
      ["정부조직법 제10조", 1],
    ]);
  });

  it("marks only valid spans by character", () => {
    expect(markSpans("가나다라", [[1, 3], [2, 9]])).toEqual([
      { text: "가", marked: false },
      { text: "나다", marked: true },
      { text: "라", marked: false },
    ]);
  });

  it("reads a Sources event and drops broken items", () => {
    const ev = toChatEvent({
      event: "Sources",
      data: JSON.stringify({ items: [{ kind: "law", label: "법 제1조", note: "", displays: ["법 제1조"], body: "본문", spans: [[0, 2], [1, 5]] }, { kind: "x" }] }),
    } as never);
    expect(ev).toEqual({ kind: "sources", items: [{ kind: "law", label: "법 제1조", note: "", displays: ["법 제1조"], body: "본문", spans: [[0, 2]], id: "", verified: false, verifyNote: "" }] });
  });
});
