import { useCallback, useEffect, useRef, useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import type { UploadSummary } from "../upload/api";
import { ask, loadHistory } from "./api";
import type { HistoryItem } from "./history";
import { BROKEN_TEXT, applyEvent, breakTurn, startTurn, type Turn } from "./turn";

export type Conversation = {
  history: HistoryItem[];
  loading: boolean;
  loadError: string | null;
  turns: Turn[];
  streaming: boolean;
  notice: string | null;
  uploads: UploadSummary[];
  submit: (question: string) => void;
  addUpload: (summary: UploadSummary) => void;
  reload: () => void;
};

export function useConversation(client: ApiClient, sessionId: string, onStreaming: (streaming: boolean) => void): Conversation {
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [uploads, setUploads] = useState<UploadSummary[]>([]);
  const nextId = useRef(1);
  const controller = useRef<AbortController | null>(null);
  const alive = useRef(true);

  const reload = useCallback(() => {
    setLoading(true);
    setLoadError(null);
    loadHistory(client, sessionId)
      .then((items) => {
        if (!alive.current) {
          return;
        }
        setHistory(items);
        setTurns([]);
        setUploads([]);
      })
      .catch((err: unknown) => {
        if (alive.current) {
          setLoadError(messageOf(err));
        }
      })
      .finally(() => {
        if (alive.current) {
          setLoading(false);
        }
      });
  }, [client, sessionId]);

  useEffect(() => {
    alive.current = true;
    reload();
    return () => {
      alive.current = false;
      controller.current?.abort();
    };
  }, [reload]);

  useEffect(() => {
    onStreaming(streaming);
  }, [streaming, onStreaming]);

  const submit = useCallback(
    (question: string) => {
      if (streaming) {
        return;
      }
      const id = nextId.current++;
      const update = (fn: (turn: Turn) => Turn) => {
        if (alive.current) {
          setTurns((list) => list.map((t) => (t.id === id ? fn(t) : t)));
        }
      };
      const abort = new AbortController();
      controller.current = abort;
      setNotice(null);
      setStreaming(true);
      setTurns((list) => [...list, startTurn(id, question)]);
      ask(client, sessionId, question, (event) => update((t) => applyEvent(t, event)), abort.signal)
        .then((end) => {
          if (end === "broken" && alive.current) {
            update(breakTurn);
            setNotice(BROKEN_TEXT);
            reload();
          }
        })
        .catch((err: unknown) => {
          if (abort.signal.aborted) {
            return;
          }
          update((t) => applyEvent(t, { kind: "error", message: messageOf(err), requestId: "" }));
        })
        .finally(() => {
          if (alive.current) {
            setStreaming(false);
          }
          if (controller.current === abort) {
            controller.current = null;
          }
        });
    },
    [client, sessionId, streaming, reload],
  );

  const addUpload = useCallback((summary: UploadSummary) => {
    setUploads((list) => [...list, summary]);
  }, []);

  return { history, loading, loadError, turns, streaming, notice, uploads, submit, addUpload, reload };
}
