import { useCallback, useEffect, useState } from "react";
import { createSession, deleteSession, listSessions, type SessionSummary } from "../features/chat/api";
import { ChatView } from "../features/chat/ChatView";
import { SessionList, sessionTitle } from "../features/chat/SessionList";
import type { ApiClient } from "../shared/api/client";
import { messageOf } from "../shared/api/errors";
import { ErrorNote } from "../shared/ui/ErrorNote";

type Props = {
  client: ApiClient;
  onStreaming: (streaming: boolean) => void;
};

export function Workspace({ client, onStreaming }: Props) {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [locked, setLocked] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const list = await listSessions(client);
    setSessions(list);
    return list;
  }, [client]);

  useEffect(() => {
    refresh().catch((err: unknown) => setError(messageOf(err)));
  }, [refresh]);

  const streaming = useCallback(
    (on: boolean) => {
      setLocked(on);
      onStreaming(on);
    },
    [onStreaming],
  );

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const id = await createSession(client);
      await refresh();
      setSelected(id);
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

  const current = sessions.find((s) => s.id === selected);

  return (
    <div className="workspace">
      <div className="workspace-side">
        <SessionList sessions={sessions} selected={selected} locked={locked} busy={busy} onSelect={setSelected} onCreate={create} onDelete={remove} />
        <ErrorNote message={error} />
      </div>
      <div className="workspace-main">
        {current ? (
          <ChatView key={current.id} client={client} sessionId={current.id} title={sessionTitle(current)} onStreaming={streaming} />
        ) : (
          <p className="muted empty">대화를 고르거나 새 대화를 만드세요.</p>
        )}
      </div>
    </div>
  );
}
