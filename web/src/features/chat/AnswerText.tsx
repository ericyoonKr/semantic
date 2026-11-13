import { type RefObject, useEffect, useRef, useState } from "react";
import { Fold } from "../../shared/ui/Fold";
import { Icon } from "../../shared/ui/Icon";
import type { Source } from "./events";
import { useSourceView } from "./sourceView";
import { type BoxTable, markSpans, splitBody, tableWidth } from "../../shared/webtext";
import { citeNames, toBlocks, toSegments } from "./rich";

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
        if (s.doc !== undefined) {
          return (
            <span key={i} className="doc-ref" title={s.doc}>
              <Icon name="doc" />
              {s.text}
            </span>
          );
        }
        const body =
          s.source === null ? (
            s.text
          ) : (
            <span
              role="button"
              tabIndex={0}
              className="cite"
              onClick={() => open(s.source ?? 0)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  open(s.source ?? 0);
                }
              }}
            >
              {s.text}
              <sup className="cite-num">{(s.source ?? 0) + 1}</sup>
            </span>
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

export function SourceDetail({ source, close, modal = false }: { source: Source; close: () => void; modal?: boolean }) {
  const closer = useRef<HTMLButtonElement>(null);
  const mark = useRef<HTMLElement>(null);
  useEffect(() => {
    if (modal) {
      closer.current?.focus();
    }
    mark.current?.scrollIntoView?.({ block: "center" });
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        close();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [close, source, modal]);
  return (
    <article className="source-detail" aria-label={source.label + " 원문"}>
      <header className="source-head">
        <div className="source-title">
          <span className="source-kind">{source.kind === "law" ? "법령" : "판례"}</span>
          <strong>{source.label}</strong>
          {source.note ? <span className="source-note">{source.note}</span> : null}
        </div>
        <button ref={closer} type="button" className="icon-button source-close" onClick={close} aria-label="원문 닫기">
          <Icon name="close" />
        </button>
      </header>
      <p className={source.verified ? "source-check is-ok" : "source-check is-bad"}>
        <Icon name={source.verified ? "check" : "alert"} />
        <span>
          {source.verified ? "DB 대조 확인" : "DB 대조 실패" + (source.verifyNote ? ": " + source.verifyNote : "")}
          {source.id ? " · " + source.id : ""}
        </span>
      </p>
      {source.spans.length === 0 ? <p className="source-hint">답에 쓴 부분을 원문에서 짚지 못했습니다.</p> : null}
      <SourceBody body={source.body} spans={source.spans} markRef={mark} />
    </article>
  );
}

function SourceModal({ source, close }: { source: Source; close: () => void }) {
  return (
    <div className="source-modal-backdrop" onClick={close}>
      <div className="source-modal" role="dialog" aria-modal="true" aria-label={source.label + " 원문"} onClick={(e) => e.stopPropagation()}>
        <SourceDetail source={source} close={close} modal />
      </div>
    </div>
  );
}

export function AnswerText({ text, streaming = false, sources = [] }: Props) {
  const view = useSourceView();
  const [local, setLocal] = useState<number | null>(null);
  const names = citeNames(sources.map((s) => (s.displays.length > 0 ? s.displays : [s.label])));
  const isShown = (i: number) => (view ? view.shown === sources[i] : local === i);
  const open = (i: number) => {
    const s = sources[i];
    if (!s) {
      return;
    }
    if (view) {
      view.open(s);
    } else {
      setLocal(local === i ? null : i);
    }
  };
  const modalSource = !view && local !== null ? sources[local] : undefined;
  return (
    <Fold long={!streaming && text.length > LONG_ANSWER_CHARS}>
      <div className="answer-text">
        <RichBlocks text={text} names={names} open={open} />
      </div>
      {sources.length > 0 ? (
        <ol className="source-list" aria-label="근거">
          {sources.map((s, i) => (
            <li key={i}>
              <button
                type="button"
                className={"source-chip" + (isShown(i) ? " is-open" : "") + (s.verified ? "" : " is-unverified")}
                title={s.label + (s.note ? " · " + s.note : "")}
                onClick={() => open(i)}
              >
                <span className="source-num">{i + 1}</span>
                <span className="source-chip-label">{s.label}</span>
                <span className="source-chip-state" aria-label={s.verified ? "DB 대조 확인" : "DB 대조 실패"}>
                  <Icon name={s.verified ? "check" : "alert"} />
                </span>
              </button>
            </li>
          ))}
        </ol>
      ) : null}
      {modalSource ? <SourceModal source={modalSource} close={() => setLocal(null)} /> : null}
    </Fold>
  );
}
