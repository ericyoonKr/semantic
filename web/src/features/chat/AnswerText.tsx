import { type RefObject, useEffect, useMemo, useRef, useState } from "react";
import { Fold } from "../../shared/ui/Fold";
import { Icon } from "../../shared/ui/Icon";
import type { Source } from "./events";
import { type DocRef, useSourceView } from "./sourceView";
import { type BoxTable, type Layered, findAll, markLayers, splitBody, tableWidth } from "../../shared/webtext";
import { pageOf } from "../upload/files";
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
  openDoc?: ((ref: DocRef) => void) | undefined;
};

function Line({ text, names, open, openDoc }: LineProps) {
  return (
    <>
      {toSegments(text, names).map((s, i) => {
        if (s.doc !== undefined) {
          const doc = s.doc;
          if (!openDoc) {
            return (
              <span key={i} className="doc-ref" title={doc}>
                <Icon name="doc" />
                {s.text}
              </span>
            );
          }
          const go = () => openDoc({ filename: doc, page: pageOf(s.text) });
          return (
            <span
              key={i}
              role="button"
              tabIndex={0}
              className="doc-ref doc-ref-link"
              title={doc + " 원문 보기"}
              onClick={go}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  go();
                }
              }}
            >
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

type Range = [number, number];

type MarkRef = RefObject<HTMLElement | null>;

function layerClass(l: Layered): string {
  return [l.cite ? "is-cite" : "", l.find ? "is-find" : "", l.current ? "is-current" : ""].filter(Boolean).join(" ");
}

function BodyText({ layers, refAt, markRef }: { layers: Layered[]; refAt: number; markRef: MarkRef }) {
  return (
    <pre className="source-body">
      {layers.map((l, i) =>
        l.cite || l.find ? (
          <mark key={i} ref={i === refAt ? markRef : undefined} className={layerClass(l)}>
            {l.text}
          </mark>
        ) : (
          <span key={i}>{l.text}</span>
        ),
      )}
    </pre>
  );
}

type BodyProps = { body: string; cites: Range[]; finds: Range[]; current: number; markRef: MarkRef };

function SourceBody({ body, cites, finds, current, markRef }: BodyProps) {
  const pieces = useMemo(() => splitBody(body), [body]);
  const target = current >= 0 ? finds[current] : undefined;
  let placed = false;
  const claim = (want: boolean) => {
    if (!want || placed) {
      return false;
    }
    placed = true;
    return true;
  };
  return (
    <div className="source-body-wrap">
      {pieces.map((p, i) => {
        const inside = (r: Range) => r[0] < p.end && r[1] > p.start;
        const local = (r: Range): Range => [Math.max(r[0], p.start) - p.start, Math.min(r[1], p.end) - p.start];
        const citesIn = cites.filter(inside);
        const findsIn = finds.filter(inside);
        const here = target ? findsIn.indexOf(target) : -1;
        if (p.kind === "table") {
          const wanted = target ? here >= 0 : citesIn.length > 0;
          return (
            <div key={i} ref={claim(wanted) ? (markRef as RefObject<HTMLDivElement>) : undefined}>
              <GridTable table={p.table} marked={citesIn.length > 0 || findsIn.length > 0} />
            </div>
          );
        }
        const layers = markLayers(p.text, citesIn.map(local), findsIn.map(local), here >= 0 ? here : null);
        const refAt = layers.findIndex((l) => (target ? l.current : l.cite));
        return <BodyText key={i} layers={layers} refAt={claim(refAt >= 0) ? refAt : -1} markRef={markRef} />;
      })}
    </div>
  );
}

type RichProps = {
  text: string;
  names?: { name: string; source: number }[];
  open?: (source: number) => void;
  openDoc?: ((ref: DocRef) => void) | undefined;
};

const NO_NAMES: { name: string; source: number }[] = [];
const IGNORE = (_source: number) => undefined;

export function RichBlocks({ text, names = NO_NAMES, open = IGNORE, openDoc }: RichProps) {
  const setShown = open;
  return (
    <>
    {toBlocks(text).map((b, i) => {
      switch (b.kind) {
        case "para":
          return (
            <p key={i}>
              <Line text={b.text} names={names} open={setShown} openDoc={openDoc} />
            </p>
          );
        case "heading":
          return (
            <p key={i} className={"answer-heading answer-heading-" + Math.min(b.level, 4)} role="heading" aria-level={Math.min(b.level + 2, 6)}>
              <Line text={b.text} names={names} open={setShown} openDoc={openDoc} />
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
                          <Line text={c} names={names} open={setShown} openDoc={openDoc} />
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
                          <Line text={c} names={names} open={setShown} openDoc={openDoc} />
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
                  <Line text={item} names={names} open={setShown} openDoc={openDoc} />
                </li>
              ))}
            </ul>
          );
      }
    })}
    </>
  );
}

type Find = { of: Source; query: string; at: number };

export function SourceDetail({ source, close, modal = false }: { source: Source; close: () => void; modal?: boolean }) {
  const closer = useRef<HTMLButtonElement>(null);
  const mark = useRef<HTMLElement>(null);
  const [find, setFind] = useState<Find>({ of: source, query: "", at: 0 });
  const live = find.of === source ? find : { of: source, query: "", at: 0 };
  const finds = useMemo(() => findAll(source.body, live.query), [source.body, live.query]);
  const count = finds.length;
  const current = count === 0 ? -1 : Math.min(live.at, count - 1);
  const hasQuery = live.query.length > 0;
  const step = (delta: number) => {
    if (count > 0) {
      setFind({ of: source, query: live.query, at: (current + delta + count) % count });
    }
  };
  useEffect(() => {
    if (modal) {
      closer.current?.focus();
    }
  }, [modal, source]);
  useEffect(() => {
    mark.current?.scrollIntoView?.({ block: "center" });
  }, [source, live.query, current]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") {
        return;
      }
      if (hasQuery) {
        setFind({ of: source, query: "", at: 0 });
      } else {
        close();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [close, source, hasQuery]);
  return (
    <article className="source-detail" aria-label={source.label + " 원문"}>
      <div className="source-top">
        <header className="source-head">
          <div className="source-title">
            <span className="source-kind">{source.norm}</span>
            <strong>{source.label}</strong>
            {source.note ? <span className="source-note">{source.note}</span> : null}
          </div>
          <button ref={closer} type="button" className="icon-button source-close" onClick={close} aria-label="원문 닫기">
            <Icon name="close" />
          </button>
        </header>
        <div className="source-find" role="search">
          <input
            type="text"
            className="source-find-input"
            value={live.query}
            placeholder="원문에서 찾기"
            aria-label="원문에서 찾기"
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setFind({ of: source, query: e.target.value, at: 0 })}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                e.preventDefault();
                step(e.shiftKey ? -1 : 1);
              }
            }}
          />
          <span className="source-find-count" aria-live="polite">
            {(current < 0 ? 0 : current + 1) + "/" + count}
          </span>
          <button type="button" className="icon-button" aria-label="이전 일치" disabled={count === 0} onClick={() => step(-1)}>
            <Icon name="up" />
          </button>
          <button type="button" className="icon-button" aria-label="다음 일치" disabled={count === 0} onClick={() => step(1)}>
            <Icon name="down" />
          </button>
        </div>
      </div>
      <p className={source.verified ? "source-check is-ok" : "source-check is-bad"}>
        <Icon name={source.verified ? "check" : "alert"} />
        <span>
          {source.verified ? "DB 대조 확인" : "DB 대조 실패" + (source.verifyNote ? ": " + source.verifyNote : "")}
          {source.id ? " · " + source.id : ""}
        </span>
      </p>
      {source.spans.length === 0 ? <p className="source-hint">답에 쓴 부분을 원문에서 짚지 못했습니다.</p> : null}
      <SourceBody body={source.body} cites={source.spans} finds={finds} current={current} markRef={mark} />
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
        <RichBlocks text={text} names={names} open={open} openDoc={view?.openDoc} />
      </div>
      {sources.length > 0 ? (
        <ol className="source-list" aria-label="근거">
          {sources.map((s, i) => (
            <li key={i}>
              <button
                type="button"
                className={"source-chip" + (isShown(i) ? " is-open" : "") + (s.verified ? "" : " is-unverified")}
                title={s.norm + " · " + s.label + (s.note ? " · " + s.note : "")}
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
