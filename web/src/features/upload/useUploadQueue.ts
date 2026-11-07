import { useCallback, useEffect, useRef, useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { uploadFile, type UploadSummary } from "./api";
import { BLOCKED_TEXT, UploadQueue, type QueueItem } from "./dropQueue";

export type UploadQueueState = {
  items: QueueItem[];
  busy: boolean;
  notice: string | null;
  offer: (files: readonly File[]) => void;
  dismiss: (id: number) => void;
};

export function useUploadQueue(client: ApiClient, sessionId: string, blocked: boolean, onUploaded: (summary: UploadSummary) => void): UploadQueueState {
  const [items, setItems] = useState<QueueItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const queue = useRef<UploadQueue | null>(null);
  const uploaded = useRef(onUploaded);

  useEffect(() => {
    uploaded.current = onUploaded;
  }, [onUploaded]);

  useEffect(() => {
    const q = new UploadQueue((file) => uploadFile(client, sessionId, file), {
      change: setItems,
      busy: setBusy,
      uploaded: (summary) => uploaded.current(summary),
    });
    queue.current = q;
    setItems([]);
    return () => {
      q.close();
      if (queue.current === q) {
        queue.current = null;
      }
    };
  }, [client, sessionId]);

  useEffect(() => {
    if (!blocked) {
      setNotice((current) => (current === BLOCKED_TEXT ? null : current));
    }
  }, [blocked]);

  const offer = useCallback(
    (files: readonly File[]) => {
      const result = queue.current?.offer(files, blocked) ?? "empty";
      setNotice(result === "blocked" ? BLOCKED_TEXT : null);
    },
    [blocked],
  );

  const dismiss = useCallback((id: number) => {
    queue.current?.dismiss(id);
  }, []);

  return { items, busy, notice, offer, dismiss };
}
