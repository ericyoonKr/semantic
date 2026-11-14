import { useCallback, useEffect, useRef, useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import type { UploadSummary } from "../upload/api";
import { type AskEnd, ask, attachLive, loadHistory, retry as retryAsk } from "./api";
import type { ChatEvent } from "./events";
import { dropLastTurn, dropPending, lastQuestion, type HistoryItem } from "./history";
import { BROKEN_TEXT, applyEvent, breakTurn, startTurn, type Turn } from "./turn";

export type Conversation = {
  history: HistoryItem[];
  loading: boolean;
  loadError: string | null;
  turns: Turn[];
  streaming: boolean;
  notice: string | null;
  uploads: UploadSummary[];
  canRetry: boolean;
  submit: (question: string) => void;
  retry: () => void;
  addUpload: (summary: UploadSummary) => void;
  reload: () => void;
};

type Go = (onEvent: (event: ChatEvent) => void, signal: AbortSignal) => Promise<AskEnd>;

export type RetryTarget = { question: string; resend: boolean };

const LIVE_KEY = 0;

export function retryTarget(turns: Turn[], history: HistoryItem[]): RetryTarget | null {
  const last = turns[turns.length - 1];
  if (last) {
    if (last.state === "streaming") {
      return null;
    }
    return { question: last.question, resend: last.state === "error" && (last.error?.requestId ?? "") === "" };
  }
  const question = lastQuestion(history);
  return question === null ? null : { question, resend: false };
}

export function useConversation(client: ApiClient, sessionId: string, onStreaming: (streaming: boolean) => void): Conversation {
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [uploads, setUploads] = useState<UploadSummary[]>([]);
  const nextId = useRef(1);
  const controllers = useRef(new Map<number, AbortController>());
  const alive = useRef(true);
  const streaming = turns.some((t) => t.state === "streaming");

  const attach = useCallback(() => {
    if (controllers.current.has(LIVE_KEY)) {
      return;
    }
    const abort = new AbortController();
    controllers.current.set(LIVE_KEY, abort);
    let id: number | null = null;
    const update = (fn: (turn: Turn) => Turn) => {
      const target = id;
      if (alive.current && target !== null) {
        setTurns((list) => list.map((t) => (t.id === target ? fn(t) : t)));
      }
    };
    attachLive(client, sessionId, (event) => {
      if (event.kind === "resume") {
        if (id !== null || !alive.current) {
          return;
        }
        const turnId = nextId.current++;
        id = turnId;
        setHistory((items) => dropPending(items, event.question));
        setTurns((list) => [...list, startTurn(turnId, event.question)]);
        return;
      }
      update((t) => applyEvent(t, event));
    }, abort.signal)
      .then((end) => {
        if (end === "broken") {
          update(breakTurn);
        }
      })
      .catch(() => {
        if (!abort.signal.aborted) {
          update(breakTurn);
        }
      })
      .finally(() => {
        if (controllers.current.get(LIVE_KEY) === abort) {
          controllers.current.delete(LIVE_KEY);
        }
      });
  }, [client, sessionId]);

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
        attach();
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
  }, [client, sessionId, attach]);

  useEffect(() => {
    alive.current = true;
    reload();
    return () => {
      alive.current = false;
      for (const abort of controllers.current.values()) {
        abort.abort();
      }
      controllers.current.clear();
    };
  }, [reload]);

  useEffect(() => {
    onStreaming(streaming);
  }, [streaming, onStreaming]);

  const run = useCallback(
    (id: number, go: Go) => {
      const update = (fn: (turn: Turn) => Turn) => {
        if (alive.current) {
          setTurns((list) => list.map((t) => (t.id === id ? fn(t) : t)));
        }
      };
      const abort = new AbortController();
      controllers.current.set(id, abort);
      setNotice(null);
      go((event) => update((t) => applyEvent(t, event)), abort.signal)
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
          if (controllers.current.get(id) === abort) {
            controllers.current.delete(id);
          }
        });
    },
    [reload],
  );

  const submit = useCallback(
    (question: string) => {
      const id = nextId.current++;
      setTurns((list) => [...list, startTurn(id, question)]);
      run(id, (onEvent, signal) => ask(client, sessionId, question, onEvent, signal));
    },
    [client, sessionId, run],
  );

  const retry = useCallback(() => {
    if (streaming) {
      return;
    }
    const target = retryTarget(turns, history);
    if (target === null) {
      return;
    }
    const id = nextId.current++;
    if (turns.length > 0) {
      setTurns((list) => [...list.slice(0, -1), startTurn(id, target.question)]);
    } else {
      setHistory((items) => dropLastTurn(items));
      setTurns([startTurn(id, target.question)]);
    }
    run(id, (onEvent, signal) =>
      target.resend ? ask(client, sessionId, target.question, onEvent, signal) : retryAsk(client, sessionId, onEvent, signal),
    );
  }, [client, sessionId, streaming, turns, history, run]);

  const addUpload = useCallback((summary: UploadSummary) => {
    setUploads((list) => [...list, summary]);
  }, []);

  const canRetry = !streaming && !loading && retryTarget(turns, history) !== null;

  return { history, loading, loadError, turns, streaming, notice, uploads, canRetry, submit, retry, addUpload, reload };
}
