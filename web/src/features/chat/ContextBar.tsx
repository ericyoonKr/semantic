import type { ContextUse } from "./events";

export function contextLabel(c: ContextUse): string {
  return `문맥 ${c.used.toLocaleString("ko-KR")} / ${c.limit.toLocaleString("ko-KR")}`;
}

export function ContextBar({ context }: { context: ContextUse | null }) {
  if (!context) {
    return null;
  }
  const ratio = Math.min(1, context.used / context.limit);
  const mark = context.summaryAt > 0 && context.summaryAt < context.limit ? context.summaryAt / context.limit : null;
  const title = mark !== null ? `요약 기준 ${context.summaryAt.toLocaleString("ko-KR")} 토큰을 넘으면 앞부분을 요약합니다` : undefined;
  return (
    <div className="context-bar" title={title}>
      <span className="context-bar-label">{contextLabel(context)}</span>
      <span className="context-bar-track" role="meter" aria-valuemin={0} aria-valuemax={context.limit} aria-valuenow={context.used} aria-label="대화 문맥 사용량">
        <span className="context-bar-fill" style={{ width: `${Math.round(ratio * 100)}%` }} />
        {mark !== null ? <span className="context-bar-mark" style={{ left: `${Math.round(mark * 100)}%` }} /> : null}
      </span>
    </div>
  );
}
