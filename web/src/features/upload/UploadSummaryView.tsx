import type { UploadSummary } from "./api";

type Props = {
  summary: UploadSummary;
};

export function UploadSummaryView({ summary }: Props) {
  return (
    <section className="upload-summary" aria-label="방금 올린 파일">
      <h3>방금 올린 파일</h3>
      <p>
        {summary.filename} · {summary.format.toUpperCase()} · {summary.pages}쪽 · 검사 결과 {summary.findingsCount}건
      </p>
      {summary.warnings.length > 0 ? (
        <ul className="warnings">
          {summary.warnings.map((w, i) => (
            <li key={i}>{w}</li>
          ))}
        </ul>
      ) : null}
      {summary.outline.length > 0 ? (
        <details>
          <summary>목차 {summary.outline.length}개</summary>
            <ol className="outline">
              {summary.outline.map((o, i) => (
                <li key={i} className={`outline-level-${Math.min(Math.max(o.level, 1), 4)}`}>
                  {o.title}
                  {o.page !== null ? <span className="page"> {o.page}쪽</span> : null}
                </li>
              ))}
            </ol>
        </details>
      ) : null}
    </section>
  );
}
