import type { SessionSummary } from "./api";

export function sessionTitle(s: SessionSummary): string {
  return s.title ?? "대화 " + s.id.slice(0, 8);
}

type Props = {
  sessions: SessionSummary[];
  selected: string | null;
  locked: boolean;
  busy: boolean;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDelete: (s: SessionSummary) => void;
};

export function SessionList({ sessions, selected, locked, busy, onSelect, onCreate, onDelete }: Props) {
  return (
    <nav className="sessions" aria-label="대화 목록">
      <h2>대화 목록</h2>
      <button type="button" onClick={onCreate} disabled={locked || busy}>
        새 대화
      </button>
      {sessions.length === 0 ? <p className="muted">대화가 없습니다.</p> : null}
      <ul>
        {sessions.map((s) => {
          const title = sessionTitle(s);
          const current = s.id === selected;
          return (
            <li key={s.id} className={current ? "session current" : "session"}>
              <button type="button" className="session-open" aria-current={current ? "page" : undefined} onClick={() => onSelect(s.id)} disabled={locked && !current}>
                {title}
              </button>
              <button type="button" className="session-delete" aria-label={title + " 지우기"} onClick={() => onDelete(s)} disabled={locked || busy}>
                지우기
              </button>
            </li>
          );
        })}
      </ul>
      {locked ? <p className="muted">답을 받는 동안에는 다른 대화로 옮길 수 없습니다.</p> : null}
    </nav>
  );
}
