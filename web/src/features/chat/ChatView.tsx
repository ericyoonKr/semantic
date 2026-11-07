import { useEffect, useRef, useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { UploadForm } from "../upload/UploadForm";
import { UploadList } from "../upload/UploadList";
import { UploadSummaryView } from "../upload/UploadSummaryView";
import { AskForm } from "./AskForm";
import { LiveTurn } from "./LiveTurn";
import { MessageList } from "./MessageList";
import { useConversation } from "./useConversation";

type Props = {
  client: ApiClient;
  sessionId: string;
  title: string;
  onStreaming: (streaming: boolean) => void;
};

export function ChatView({ client, sessionId, title, onStreaming }: Props) {
  const c = useConversation(client, sessionId, onStreaming);
  const [uploading, setUploading] = useState(false);
  const end = useRef<HTMLLIElement>(null);
  const last = c.uploads.length > 0 ? c.uploads[c.uploads.length - 1] : undefined;
  const uploadLines = [
    ...c.history.filter((h) => h.role === "upload").map((h) => h.text),
    ...c.uploads.map((u) => u.filename + ", " + u.pages + "쪽"),
  ];

  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [c.turns, c.history]);

  return (
    <div className="chat">
      <section className="chat-main" aria-label={title}>
        <h2>{title}</h2>
        {c.notice ? (
          <p className="notice notice-warn" role="alert">
            {c.notice}
          </p>
        ) : null}
        {c.loading ? <p className="muted" role="status">대화 기록을 불러오는 중</p> : null}
        <ErrorNote message={c.loadError} />
        {c.loadError ? (
          <button type="button" onClick={c.reload}>
            다시 불러오기
          </button>
        ) : null}
        <ol className="messages">
          <MessageList items={c.history} />
          {c.turns.map((t) => (
            <LiveTurn key={t.id} turn={t} />
          ))}
          <li ref={end} className="messages-end" aria-hidden="true" />
        </ol>
        <AskForm sessionId={sessionId} disabled={c.streaming || uploading || c.loading} onAsk={c.submit} />
      </section>
      <aside className="chat-side" aria-label="파일">
        <UploadForm client={client} sessionId={sessionId} disabled={c.streaming} onBusy={setUploading} onUploaded={c.addUpload} />
        {last ? <UploadSummaryView summary={last} /> : null}
        <UploadList lines={uploadLines} />
      </aside>
    </div>
  );
}
