export type Block =
  | { kind: "para"; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "heading"; level: number; text: string }
  | { kind: "rule" }
  | { kind: "table"; header: string[] | null; rows: string[][] };

export type Segment = { text: string; bold: boolean; source: number | null };

const LIST_LINE = /^\s*[*\-•]\s+(.*)$/;
const HEADING_LINE = /^\s*(#{1,6})\s+(.*?)\s*#*\s*$/;
const RULE_LINE = /^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/;
const TABLE_LINE = /^\s*\|.*\|\s*$/;
const TABLE_SEPARATOR = /^:?-{2,}:?$/;

function cells(line: string): string[] {
  return line.trim().slice(1, -1).split("|").map((c) => c.trim());
}

export function toBlocks(text: string): Block[] {
  const out: Block[] = [];
  for (const line of text.split("\n")) {
    const last = out[out.length - 1];
    if (TABLE_LINE.test(line)) {
      const row = cells(line);
      if (last && last.kind === "table") {
        if (row.every((c) => TABLE_SEPARATOR.test(c))) {
          if (last.header === null && last.rows.length === 1) {
            last.header = last.rows.pop() ?? null;
          }
        } else {
          last.rows.push(row);
        }
      } else {
        out.push({ kind: "table", header: null, rows: [row] });
      }
      continue;
    }
    if (line.trim() === "" && last && last.kind === "table") {
      continue;
    }
    const heading = HEADING_LINE.exec(line);
    if (heading?.[1] && heading[2]) {
      out.push({ kind: "heading", level: heading[1].length, text: heading[2] });
      continue;
    }
    if (RULE_LINE.test(line)) {
      out.push({ kind: "rule" });
      continue;
    }
    const item = LIST_LINE.exec(line)?.[1];
    if (item !== undefined) {
      if (last && last.kind === "list") {
        last.items.push(item);
      } else {
        out.push({ kind: "list", items: [item] });
      }
    } else if (line.trim() !== "") {
      out.push({ kind: "para", text: line });
    }
  }
  return out;
}

function boldPieces(text: string): { text: string; bold: boolean }[] {
  const parts = text.split("**");
  if (parts.length % 2 === 0) {
    return [{ text, bold: false }];
  }
  return parts.map((t, i) => ({ text: t, bold: i % 2 === 1 })).filter((p) => p.text !== "");
}

export function citeNames(displays: string[][]): { name: string; source: number }[] {
  const out: { name: string; source: number }[] = [];
  displays.forEach((names, source) => {
    for (const name of names) {
      if (name !== "" && !out.some((o) => o.name === name)) {
        out.push({ name, source });
      }
    }
  });
  return out.sort((a, b) => b.name.length - a.name.length);
}

function citePieces(text: string, bold: boolean, names: { name: string; source: number }[]): Segment[] {
  const out: Segment[] = [];
  let rest = text;
  while (rest !== "") {
    let best: { at: number; name: string; source: number } | null = null;
    for (const n of names) {
      const at = rest.indexOf(n.name);
      if (at >= 0 && (best === null || at < best.at)) {
        best = { at, name: n.name, source: n.source };
      }
    }
    if (best === null) {
      out.push({ text: rest, bold, source: null });
      break;
    }
    if (best.at > 0) {
      out.push({ text: rest.slice(0, best.at), bold, source: null });
    }
    out.push({ text: best.name, bold, source: best.source });
    rest = rest.slice(best.at + best.name.length);
  }
  return out;
}

export function toSegments(text: string, names: { name: string; source: number }[]): Segment[] {
  return boldPieces(text).flatMap((p) => citePieces(p.text, p.bold, names));
}

export type Marked = { text: string; marked: boolean };

export function markSpans(body: string, spans: [number, number][]): Marked[] {
  const chars = Array.from(body);
  const out: Marked[] = [];
  let at = 0;
  for (const [start, end] of [...spans].sort((a, b) => a[0] - b[0])) {
    if (start < at || end > chars.length) {
      continue;
    }
    if (start > at) {
      out.push({ text: chars.slice(at, start).join(""), marked: false });
    }
    out.push({ text: chars.slice(start, end).join(""), marked: true });
    at = end;
  }
  if (at < chars.length) {
    out.push({ text: chars.slice(at).join(""), marked: false });
  }
  return out;
}
