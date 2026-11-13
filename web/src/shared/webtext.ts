import init, { initSync, mark_spans, plain_math, split_body } from "../wasm/webtext.js";

export type BoxTable = { head: string[][]; rows: string[][] };

export type BodyPiece =
  | { kind: "text"; text: string; start: number; end: number }
  | { kind: "table"; table: BoxTable; start: number; end: number };

export type Marked = { text: string; marked: boolean };

export async function loadWebtext(): Promise<void> {
  await init();
}

export function loadWebtextFrom(bytes: BufferSource): void {
  initSync({ module: bytes });
}

export function splitBody(body: string): BodyPiece[] {
  return JSON.parse(split_body(body)) as BodyPiece[];
}

export function markSpans(body: string, spans: [number, number][]): Marked[] {
  return JSON.parse(mark_spans(body, JSON.stringify(spans))) as Marked[];
}

export function plainMath(text: string): string {
  return plain_math(text);
}

export function tableWidth(table: BoxTable): number {
  return Math.max(1, ...table.head.map((r) => r.length), ...table.rows.map((r) => r.length));
}
