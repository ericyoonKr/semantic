import { useState } from "react";
import { Fold } from "../../shared/ui/Fold";
import type { Source } from "./events";
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

function SourcePanel({ source, close }: { source: Source; close: () => void }) {
  return (
    <aside className="source-panel" aria-label="근거 원문">
      <header className="source-head">
        <span className="source-kind">{source.kind === "law" ? "법령" : "판례"}</span>
        <strong>{source.label}</strong>
        {source.note ? <span className="source-note">{source.note}</span> : null}
        <span className={source.verified ? "source-check is-ok" : "source-check is-bad"}>
          {source.verified ? "DB 대조 확인" : "DB 대조 실패" + (source.verifyNote ? ": " + source.verifyNote : "")}
          {source.id ? " · ID " + source.id : ""}
        </span>
        <button type="button" className="source-close" onClick={close} aria-label="닫기">
          ×
        </button>
      </header>
      {source.spans.length === 0 ? <p className="source-hint">답에 쓴 부분을 원문에서 짚지 못했습니다.</p> : null}
      <pre className="source-body">
        {markSpans(source.body, source.spans).map((m, i) => (m.marked ? <mark key={i}>{m.text}</mark> : <span key={i}>{m.text}</span>))}
      </pre>
    </aside>
  );
}

export function AnswerText({ text, streaming = false, sources = [] }: Props) {
  const [shown, setShown] = useState<number | null>(null);
  const names = citeNames(sources.map((s) => s.displays.length > 0 ? s.displays : [s.label]));
  const source = shown === null ? undefined : sources[shown];
  return (
    <Fold long={!streaming && text.length > LONG_ANSWER_CHARS}>
      <div className="answer-text">
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
