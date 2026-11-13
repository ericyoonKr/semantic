import { type RefObject, useEffect, useRef, useState } from "react";
import { Fold } from "../../shared/ui/Fold";
import type { Source } from "./events";
import { type BoxTable, splitBody, tableWidth } from "./boxtable";
import { citeNames, markSpans, toBlocks, toSegments } from "./rich";

export const LONG_ANSWER_CHARS = 1500;

type Props = {
  text: string;
  streaming?: boolean;
  sources?: Source[];
};

type LineProps = {
  text: string;
  names: { name: string; source: number }[];
  open: (source: number) => void;
};

function Line({ text, names, open }: LineProps) {
  return (
    <>
      {toSegments(text, names).map((s, i) => {
        const body =
          s.source === null ? (
            s.text
          ) : (
            <button type="button" className="cite" onClick={() => open(s.source ?? 0)}>
              {s.text}
            </button>
          );
        return s.bold ? <strong key={i}>{body}</strong> : <span key={i}>{body}</span>;
      })}
    </>
  );
}

function GridRow({ row, width, head }: { row: string[]; width: number; head: boolean }) {
  return (
    <tr>
      {row.map((c, j) => {
        const span = j === row.length - 1 ? width - row.length + 1 : 1;
        return head ? (
          <th key={j} scope="col" colSpan={span}>
            {c}
          </th>
        ) : (
          <td key={j} colSpan={span}>
            {c}
          </td>
        );
      })}
    </tr>
  );
}

function GridTable({ table, marked = false }: { table: BoxTable; marked?: boolean }) {
  const width = tableWidth(table);
  return (
    <div className={"answer-table-wrap grid-table-wrap" + (marked ? " is-marked" : "")}>
      <table className="answer-table grid-table">
        {table.head.length > 0 ? (
          <thead>
            {table.head.map((row, r) => (
              <GridRow key={r} row={row} width={width} head />
            ))}
          </thead>
        ) : null}
        <tbody>
          {table.rows.map((row, r) => (
            <GridRow key={r} row={row} width={width} head={false} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function BodyText({ text, spans, markRef }: { text: string; spans: [number, number][]; markRef: RefObject<HTMLElement | null> }) {
  return (
    <pre className="source-body">
      {markSpans(text, spans).map((m, i) =>
        m.marked ? (
          <mark key={i} ref={markRef}>
            {m.text}
          </mark>
        ) : (
          <span key={i}>{m.text}</span>
        ),
      )}
    </pre>
  );
}

function SourceBody({ body, spans, markRef }: { body: string; spans: [number, number][]; markRef: RefObject<HTMLElement | null> }) {
  return (
    <div className="source-body-wrap">
      {splitBody(body).map((p, i) => {
        const inside = spans.filter(([s, e]) => s < p.end && e > p.start);
        if (p.kind === "table") {
          return inside.length > 0 ? (
            <div key={i} ref={markRef as RefObject<HTMLDivElement>}>
              <GridTable table={p.table} marked />
            </div>
          ) : (
            <GridTable key={i} table={p.table} />
          );
        }
        const local = inside.map(([s, e]): [number, number] => [Math.max(s, p.start) - p.start, Math.min(e, p.end) - p.start]);
        return <BodyText key={i} text={p.text} spans={local} markRef={markRef} />;
      })}
    </div>
  );
}

type RichProps = {
  text: string;
  names?: { name: string; source: number }[];
  open?: (source: number) => void;
};

const NO_NAMES: { name: string; source: number }[] = [];
const IGNORE = (_source: number) => undefined;

export function RichBlocks({ text, names = NO_NAMES, open = IGNORE }: RichProps) {
  const setShown = open;
  return (
    <>
    {toBlocks(text).map((b, i) => {
      switch (b.kind) {
        case "para":
          return (
            <p key={i}>
              <Line text={b.text} names={names} open={setShown} />
            </p>
          );
        case "heading":
          return (
            <p key={i} className={"answer-heading answer-heading-" + Math.min(b.level, 4)} role="heading" aria-level={Math.min(b.level + 2, 6)}>
              <Line text={b.text} names={names} open={setShown} />
            </p>
          );
        case "rule":
          return <hr key={i} className="answer-rule" />;
        case "table":
          return (
            <div key={i} className="answer-table-wrap">
              <table className="answer-table">
                {b.header ? (
                  <thead>
                    <tr>
                      {b.header.map((c, j) => (
                        <th key={j} scope="col">
                          <Line text={c} names={names} open={setShown} />
                        </th>
                      ))}
                    </tr>
                  </thead>
                ) : null}
                <tbody>
                  {b.rows.map((row, r) => (
                    <tr key={r}>
                      {row.map((c, j) => (
                        <td key={j}>
                          <Line text={c} names={names} open={setShown} />
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        case "grid":
          return <GridTable key={i} table={b} />;
        case "list":
          return (
            <ul key={i}>
              {b.items.map((item, j) => (
                <li key={j}>
                  <Line text={item} names={names} open={setShown} />
                </li>
              ))}
            </ul>
          );
      }
    })}
    </>
  );
}

function SourcePanel({ source, close }: { source: Source; close: () => void }) {
  const closer = useRef<HTMLButtonElement>(null);
  const mark = useRef<HTMLElement>(null);
  useEffect(() => {
    closer.current?.focus();
    mark.current?.scrollIntoView?.({ block: "center" });
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        close();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [close, source]);
  return (
    <div className="source-modal-backdrop" onClick={close}>
    <aside className="source-panel source-modal" role="dialog" aria-modal="true" aria-label={source.label + " 원문"} onClick={(e) => e.stopPropagation()}>
      <header className="source-head">
        <span className="source-kind">{source.kind === "law" ? "법령" : "판례"}</span>
        <strong>{source.label}</strong>
        {source.note ? <span className="source-note">{source.note}</span> : null}
        <span className={source.verified ? "source-check is-ok" : "source-check is-bad"}>
          {source.verified ? "DB 대조 확인" : "DB 대조 실패" + (source.verifyNote ? ": " + source.verifyNote : "")}
          {source.id ? " · ID " + source.id : ""}
        </span>
        <button ref={closer} type="button" className="source-close" onClick={close} aria-label="닫기">
          ×
        </button>
      </header>
      {source.spans.length === 0 ? <p className="source-hint">답에 쓴 부분을 원문에서 짚지 못했습니다.</p> : null}
      <SourceBody body={source.body} spans={source.spans} markRef={mark} />
    </aside>
    </div>
  );
}

export function AnswerText({ text, streaming = false, sources = [] }: Props) {
  const [shown, setShown] = useState<number | null>(null);
  const names = citeNames(sources.map((s) => s.displays.length > 0 ? s.displays : [s.label]));
  const source = shown === null ? undefined : sources[shown];
  return (
    <Fold long={!streaming && text.length > LONG_ANSWER_CHARS}>
      <div className="answer-text">
        <RichBlocks text={text} names={names} open={setShown} />
      </div>
      {sources.length > 0 ? (
        <div className="source-list" aria-label="근거">
          <span className="source-list-title">근거</span>
          {sources.map((s, i) => (
            <button key={i} type="button" className={"source-chip" + (shown === i ? " is-open" : "") + (s.verified ? "" : " is-unverified")} onClick={() => setShown(shown === i ? null : i)}>
              <span aria-label={s.verified ? "DB 대조 확인" : "DB 대조 실패"}>{s.verified ? "✓ " : "! "}</span>
              {s.label}
              {s.note ? <small> {s.note}</small> : null}
            </button>
          ))}
        </div>
      ) : null}
      {source ? <SourcePanel source={source} close={() => setShown(null)} /> : null}
    </Fold>
  );
}
