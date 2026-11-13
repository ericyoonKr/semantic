export type BoxTable = { head: string[][]; rows: string[][] };

export type BodyPiece =
  | { kind: "text"; text: string; start: number; end: number }
  | { kind: "table"; table: BoxTable; start: number; end: number };

const TOP = /^\s*┏/;
const BOTTOM = /^\s*┗/;
const BODY_START = /^\s*[┠┣]/;
const WORDS = /[\p{L}\p{N}]/u;
const RULE_CHARS = /[─━]/;

export function isBoxTop(line: string): boolean {
  return TOP.test(line);
}

export function isBoxBottom(line: string): boolean {
  return BOTTOM.test(line);
}

function isSeparator(line: string): boolean {
  return RULE_CHARS.test(line) && !WORDS.test(line);
}

function lineCells(line: string): string[] {
  const inner = line.trim().replace(/^[┃│]/, "").replace(/[┃│]$/, "");
  return inner.split(/[│┃]/).map((c) => c.trim());
}

function rowOf(group: string[][]): string[] {
  const width = Math.max(...group.map((g) => g.length));
  const row: string[] = [];
  for (let j = 0; j < width; j++) {
    row.push(group.map((g) => g[j] ?? "").filter((c) => c !== "").join("\n"));
  }
  return row;
}

export function parseBoxTable(lines: string[]): BoxTable {
  const head: string[][] = [];
  const rows: string[][] = [];
  let inBody = !lines.some((l) => BODY_START.test(l));
  let group: string[][] = [];
  const flush = () => {
    if (group.length > 0) {
      const row = rowOf(group);
      if (row.some((c) => c !== "")) {
        (inBody ? rows : head).push(row);
      }
    }
    group = [];
  };
  for (const line of lines) {
    if (isBoxTop(line) || isBoxBottom(line)) {
      flush();
      continue;
    }
    if (isSeparator(line)) {
      flush();
      if (BODY_START.test(line)) {
        inBody = true;
      }
      continue;
    }
    const cells = lineCells(line);
    if (cells.every((c) => c === "")) {
      flush();
      continue;
    }
    group.push(cells);
  }
  flush();
  return { head, rows };
}

export function tableWidth(table: BoxTable): number {
  return Math.max(1, ...table.head.map((r) => r.length), ...table.rows.map((r) => r.length));
}

export function splitBody(body: string): BodyPiece[] {
  const out: BodyPiece[] = [];
  let text: string[] = [];
  let textStart = 0;
  let box: string[] | null = null;
  let boxStart = 0;
  let at = 0;
  const pushText = (end: number) => {
    if (text.length > 0) {
      out.push({ kind: "text", text: text.join(""), start: textStart, end });
    }
    text = [];
  };
  const pushBox = (end: number) => {
    if (box !== null) {
      out.push({ kind: "table", table: parseBoxTable(box), start: boxStart, end });
    }
    box = null;
  };
  const lines = body.split("\n");
  lines.forEach((line, i) => {
    const piece = i < lines.length - 1 ? line + "\n" : line;
    const len = Array.from(piece).length;
    if (box === null && isBoxTop(line)) {
      pushText(at);
      box = [];
      boxStart = at;
    }
    if (box !== null) {
      box.push(line);
      at += len;
      if (isBoxBottom(line)) {
        pushBox(at);
        textStart = at;
      }
      return;
    }
    if (text.length === 0) {
      textStart = at;
    }
    text.push(piece);
    at += len;
  });
  pushBox(at);
  pushText(at);
  return out;
}
