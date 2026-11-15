import { describe, expect, it } from "vitest";
import { findAll, markLayers } from "../src/shared/webtext";

describe("원문 찾기", () => {
  it("공백과 줄바꿈을 무시하고 원문 글자 위치로 돌려준다(wasm)", () => {
    const body = "영업 정지 1개월\n어느 하\n  나에 해당";
    expect(findAll(body, "영업정지")).toEqual([[0, 5]]);
    expect(findAll(body, "하나에")).toEqual([[13, 19]]);
    expect(Array.from(body).slice(13, 19).join("")).toBe("하\n  나에");
    expect(findAll("영업정지", "영업 정지")).toEqual([[0, 4]]);
  });

  it("대소문자를 무시하고 빈 질의는 비어 있다", () => {
    expect(findAll("Abc ABC", "aBC")).toEqual([
      [0, 3],
      [4, 7],
    ]);
    expect(findAll("아무 글", "")).toEqual([]);
    expect(findAll("아무 글", " \n")).toEqual([]);
  });

  it("겹치지 않게 앞에서부터 찾는다", () => {
    expect(findAll("aaaa", "aa")).toEqual([
      [0, 2],
      [2, 4],
    ]);
  });
});

describe("두 겹 표시", () => {
  it("겹치는 조각에 두 층을 달고 현재 일치를 표시한다(wasm)", () => {
    expect(markLayers("가나다라마", [[1, 4]], [[3, 5]], 0)).toEqual([
      { text: "가", cite: false, find: false, current: false },
      { text: "나다", cite: true, find: false, current: false },
      { text: "라", cite: true, find: true, current: true },
      { text: "마", cite: false, find: true, current: true },
    ]);
  });

  it("현재 번호가 없으면 어느 조각도 현재가 아니고 범위 밖은 무시한다", () => {
    const got = markLayers("가나다", [[0, 9], [2, 1]], [[1, 2]], null);
    expect(got.map((l) => l.text)).toEqual(["가", "나", "다"]);
    expect(got.map((l) => l.cite)).toEqual([false, false, false]);
    expect(got.map((l) => l.find)).toEqual([false, true, false]);
    expect(got.some((l) => l.current)).toBe(false);
  });
});
