import { useCallback, useEffect, useState, type ReactNode } from "react";
import { createSession, deleteSession, listSessions, renameSession, type SessionSummary } from "../features/chat/api";
import { ChatView } from "../features/chat/ChatView";
import { SessionList, sessionTitle } from "../features/chat/SessionList";
import type { ApiClient } from "../shared/api/client";
import { messageOf } from "../shared/api/errors";
import { ErrorNote } from "../shared/ui/ErrorNote";
import { Icon } from "../shared/ui/Icon";

type Props = {
  client: ApiClient;
  onStreaming: (streaming: boolean) => void;
  account: ReactNode;
};

const SELECTED_KEY = "semantic.selected-session";

function readSelected(): string | null {
  try {
    return window.localStorage.getItem(SELECTED_KEY);
  } catch {
    return null;
  }
}

function writeSelected(id: string | null) {
  try {
    if (id === null) {
      window.localStorage.removeItem(SELECTED_KEY);
    } else {
      window.localStorage.setItem(SELECTED_KEY, id);
    }
  } catch {
    return;
  }
}

export function Workspace({ client, onStreaming, account }: Props) {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(readSelected);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [navOpen, setNavOpen] = useState(false);

  const refresh = useCallback(async () => {
    const list = await listSessions(client);
    setSessions(list);
    return list;
  }, [client]);

  useEffect(() => {
    refresh()
      .then((list) => setSelected((id) => (id !== null && list.some((s) => s.id === id) ? id : null)))
      .catch((err: unknown) => setError(messageOf(err)));
  }, [refresh]);

  useEffect(() => {
    writeSelected(selected);
  }, [selected]);

  const streaming = useCallback(
    (on: boolean) => {
      onStreaming(on);
      if (!on) {
        refresh().catch(() => undefined);
      }
    },
    [onStreaming, refresh],
  );

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const id = await createSession(client);
      await refresh();
      setSelected(id);
      setNavOpen(false);
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  async function remove(s: SessionSummary) {
    if (!window.confirm(sessionTitle(s) + "을(를) 지울까요? 지운 대화와 올린 파일은 되살릴 수 없습니다.")) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await deleteSession(client, s.id);
      if (selected === s.id) {
        setSelected(null);
      }
      await refresh();
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  async function rename(s: SessionSummary, title: string) {
    setBusy(true);
    setError(null);
    try {
      await renameSession(client, s.id, title);
      await refresh();
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  const current = sessions.find((s) => s.id === selected);

  function choose(id: string) {
    setSelected(id);
    setNavOpen(false);
  }

  const nav = (
    <button type="button" className="icon-button nav-toggle" aria-label="대화 목록" aria-expanded={navOpen} aria-controls="sidebar" onClick={() => setNavOpen(!navOpen)}>
      <Icon name="menu" />
    </button>
  );

  return (
    <div className={navOpen ? "shell nav-open" : "shell"}>
      <aside id="sidebar" className="sidebar">
        <div className="brand-row">
          <span className="brand-mark" aria-hidden="true">
            <Icon name="scale" />
          </span>
          <h1>법률 질의</h1>
          <button type="button" className="icon-button nav-close" aria-label="대화 목록 닫기" onClick={() => setNavOpen(false)}>
            <Icon name="close" />
          </button>
        </div>
        <SessionList sessions={sessions} selected={selected} busy={busy} onSelect={choose} onCreate={create} onDelete={remove} onRename={rename} />
        <div className="sidebar-error">
          <ErrorNote message={error} />
        </div>
        <div className="account">{account}</div>
      </aside>
      {navOpen ? <button type="button" className="nav-backdrop" aria-label="대화 목록 닫기" tabIndex={-1} onClick={() => setNavOpen(false)} /> : null}
      <main className="main">
        {current ? (
          <ChatView key={current.id} client={client} sessionId={current.id} title={sessionTitle(current)} onStreaming={streaming} nav={nav} />
        ) : (
          <div className="chat">
            <header className="chat-header">
              {nav}
              <h2>법률 질의</h2>
            </header>
            <div className="chat-scroll">
              <div className="chat-column">
                <div className="chat-empty">
                  <p className="chat-empty-title">대화를 고르거나 새 대화를 만드세요.</p>
                  <button type="button" className="button-primary" onClick={create} disabled={busy}>
                    <Icon name="plus" />
                    <span>새 대화</span>
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
