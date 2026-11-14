import type { ContextUse } from "./events";

export function contextLabel(c: ContextUse): string {
  return `문맥 ${c.used.toLocaleString("ko-KR")} / ${c.limit.toLocaleString("ko-KR")}`;
}

export function contextPercent(c: ContextUse): number {
  return Math.min(100, Math.round((c.used / c.limit) * 100));
}

export function ContextBar({ context }: { context: ContextUse | null }) {
  if (!context) {
    return (
      <div className="context-bar is-empty">
        <span className="context-bar-label">문맥 사용량</span>
        <span className="context-bar-track" aria-hidden="true" />
        <span className="context-bar-pct">첫 답이 끝나면 표시</span>
      </div>
    );
  }
  const pct = contextPercent(context);
  const mark = context.summaryAt > 0 && context.summaryAt < context.limit ? Math.round((context.summaryAt / context.limit) * 100) : null;
  const title = mark !== null ? `요약 기준 ${context.summaryAt.toLocaleString("ko-KR")} 토큰을 넘으면 앞부분을 요약합니다` : undefined;
  return (
    <div className="context-bar" title={title}>
      <span className="context-bar-label">{contextLabel(context)}</span>
      <span className="context-bar-track" role="meter" aria-valuemin={0} aria-valuemax={context.limit} aria-valuenow={context.used} aria-label="대화 문맥 사용량">
        <span className="context-bar-fill" style={{ width: `${pct}%` }} />
        {mark !== null ? <span className="context-bar-mark" style={{ left: `${mark}%` }} /> : null}
      </span>
      <span className="context-bar-pct">{pct}%</span>
    </div>
  );
}
