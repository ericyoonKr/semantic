import { useRef, useState, type KeyboardEvent } from "react";
import { Icon } from "../../shared/ui/Icon";
import { TITLE_MAX_CHARS, type SessionSummary } from "./api";

export function sessionTitle(s: SessionSummary): string {
  return s.title ?? "대화 " + s.id.slice(0, 8);
}

export function cleanTitle(raw: string): string | null {
  const title = raw.split(/\s+/).filter((w) => w !== "").join(" ");
  return title !== "" && [...title].length <= TITLE_MAX_CHARS ? title : null;
}

type Props = {
  sessions: SessionSummary[];
  selected: string | null;
  locked: boolean;
  busy: boolean;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDelete: (s: SessionSummary) => void;
  onRename: (s: SessionSummary, title: string) => void;
};

type EditProps = {
  session: SessionSummary;
  done: () => void;
  onRename: (s: SessionSummary, title: string) => void;
};

function TitleEditor({ session, done, onRename }: EditProps) {
  const [draft, setDraft] = useState(sessionTitle(session));
  const title = cleanTitle(draft);
  const finished = useRef(false);

  function save() {
    if (finished.current) {
      return;
    }
    finished.current = true;
    if (title !== null && title !== sessionTitle(session)) {
      onRename(session, title);
    }
    done();
  }

  function key(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      save();
    } else if (e.key === "Escape") {
      finished.current = true;
      done();
    }
  }

  return (
    <input
      className="session-title-input"
      aria-label="대화 이름"
      value={draft}
      maxLength={TITLE_MAX_CHARS}
      autoFocus
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={key}
      onBlur={save}
    />
  );
}

export function SessionList({ sessions, selected, locked, busy, onSelect, onCreate, onDelete, onRename }: Props) {
  const [editing, setEditing] = useState<string | null>(null);
  return (
    <nav className="sessions" aria-label="대화 목록">
      <div className="sessions-actions">
        <button type="button" className="new-session" onClick={onCreate} disabled={locked || busy}>
          <Icon name="plus" />
          <span>새 대화</span>
        </button>
      </div>
      <h2 className="sessions-heading">대화 목록</h2>
      <div className="sessions-scroll">
        {sessions.length === 0 ? <p className="muted sessions-empty">대화가 없습니다.</p> : null}
        <ul>
          {sessions.map((s) => {
            const title = sessionTitle(s);
            const current = s.id === selected;
            return (
              <li key={s.id} className={current ? "session current" : "session"}>
                {editing === s.id ? (
                  <TitleEditor session={s} done={() => setEditing(null)} onRename={onRename} />
                ) : (
                  <>
                    <button type="button" className="session-open" aria-current={current ? "page" : undefined} onClick={() => onSelect(s.id)} disabled={locked && !current}>
                      <span className="session-dot" aria-hidden="true" />
                      <span className="session-title">{title}</span>
                    </button>
                    <button type="button" className="session-rename" aria-label={title + " 이름 바꾸기"} title="이름 바꾸기" onClick={() => setEditing(s.id)} disabled={busy}>
                      <Icon name="pen" />
                    </button>
                    <button type="button" className="session-delete" aria-label={title + " 지우기"} title="지우기" onClick={() => onDelete(s)} disabled={locked || busy}>
                      <Icon name="trash" />
                    </button>
                  </>
                )}
              </li>
            );
          })}
        </ul>
        {locked ? <p className="muted sessions-empty">답을 받는 동안에는 다른 대화로 옮길 수 없습니다.</p> : null}
      </div>
    </nav>
  );
}
