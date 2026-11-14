import { describe, expect, it } from "vitest";
import { toBlocks } from "../src/features/chat/rich";
import { plainMath, splitBody } from "../src/shared/webtext";

const BOX = [
  "┏━━━━━━━━┯━━━━━━┯━━━━━━━━━━━━━━━━━┓",
  "┃위반사항        │근거 법령   │행정처분기준                      ┃",
  "┃                │            ├─────┬─────┬─────┨",
  "┃                │            │1차 위반  │2차 위반  │3차 위반  ┃",
  "┠────────┼──────┼─────┼─────┼─────┨",
  "┃                │            │          │          │          ┃",
  "┃1. 법 제4조를   │법 제72조   │영업정지  │영업정지  │영업허가  ┃",
  "┃  가. 썩거나    │및 법 제75  │1개월과   │3개월과   │·등록취  ┃",
  "┃                │조          │          │          │소        ┃",
  "┃                │            │          │          │          ┃",
  "┃  나. 설익어서  │            │영업정지  │영업정지  │영업정지  ┃",
  "┃                │            │15일과    │1개월과   │3개월과   ┃",
  "┗━━━━━━━━┷━━━━━━┷━━━━━━━━━━━━━━━━━┛",
];

describe("상자 표", () => {
  it("머리줄과 행을 칸으로 나눈다(wasm)", () => {
    const [piece] = splitBody(BOX.join("\n"));
    if (piece?.kind !== "table") {
      throw new Error("표로 나뉘지 않음");
    }
    const t = piece.table;
    expect(t.head).toEqual([
      ["위반사항", "근거 법령", "행정처분기준"],
      ["", "", "1차 위반", "2차 위반", "3차 위반"],
    ]);
    expect(t.rows).toHaveLength(2);
    expect(t.rows[0]).toEqual(["1. 법 제4조를\n가. 썩거나", "법 제72조 및 법 제75조", "영업정지 1개월과", "영업정지 3개월과", "영업허가·등록취소"]);
    expect(t.rows[1]?.[2]).toBe("영업정지 15일과");
  });

  it("원문을 글과 표로 나누고 위치를 지킨다", () => {
    const body = "앞 글\n" + BOX.join("\n") + "\n뒤 글";
    const pieces = splitBody(body);
    expect(pieces.map((p) => p.kind)).toEqual(["text", "table", "text"]);
    const total = Array.from(body).length;
    expect(pieces[0]?.start).toBe(0);
    expect(pieces[2]?.end).toBe(total);
    expect(pieces[1]?.end).toBe(pieces[2]?.start);
    const last = pieces[2];
    expect(last?.kind === "text" && last.text).toBe("뒤 글");
  });

  it("답 속 상자 표도 표 블록이 된다", () => {
    const blocks = toBlocks("설명\n" + BOX.join("\n") + "\n끝");
    expect(blocks.map((b) => b.kind)).toEqual(["para", "grid", "para"]);
  });
});

describe("수식 기호", () => {
  it("화살표 명령을 기호로 바꾸고 모르는 명령은 둔다", () => {
    expect(plainMath("영업정지 $\\rightarrow$ 시·군·구청")).toBe("영업정지 → 시·군·구청");
    expect(plainMath("A \\Rightarrow B, 3 $\\times$ 2")).toBe("A ⇒ B, 3 × 2");
    expect(plainMath("$\\alpha$ 그대로")).toBe("$\\alpha$ 그대로");
  });
});

describe("가는 선 상자 표", () => {
  it("첫 묶음을 머리줄로 삼고 합쳐진 칸을 남긴다(wasm)", () => {
    const light = [
      "┌────────┬──────┬──────┐",
      "│비위의 유형     │고의        │경과실      │",
      "├────────┼──────┼──────┤",
      "│  가. 회계질서  │파면        │감봉∼견책  │",
      "│문란            │            │            │",
      "├────────┼──────┴──────┤",
      "│2. 청렴의무 위반│별표 2와 같음            │",
      "└────────┴─────────────┘",
    ];
    const [piece] = splitBody(light.join("\n"));
    if (piece?.kind !== "table") {
      throw new Error("표로 나뉘지 않음");
    }
    expect(piece.table.head).toEqual([["비위의 유형", "고의", "경과실"]]);
    expect(piece.table.rows).toEqual([
      ["가. 회계질서문란", "파면", "감봉∼견책"],
      ["2. 청렴의무 위반", "별표 2와 같음"],
    ]);
  });
});
