import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ApiClient } from "../../shared/api/client";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { Icon } from "../../shared/ui/Icon";
import { DropZone } from "../upload/DropZone";
import { FileChips } from "../upload/FileChips";
import { UploadForm } from "../upload/UploadForm";
import { UploadList } from "../upload/UploadList";
import { UploadSummaryView } from "../upload/UploadSummaryView";
import { useUploadQueue } from "../upload/useUploadQueue";
import { AskForm } from "./AskForm";
import { LiveTurn } from "./LiveTurn";
import { MessageList } from "./MessageList";
import { useConversation } from "./useConversation";

type Props = {
  client: ApiClient;
  sessionId: string;
  title: string;
  onStreaming: (streaming: boolean) => void;
  nav?: ReactNode;
};

export function ChatView({ client, sessionId, title, onStreaming, nav }: Props) {
  const c = useConversation(client, sessionId, onStreaming);
  const files = useUploadQueue(client, sessionId, c.streaming, c.addUpload);
  const [filesOpen, setFilesOpen] = useState(false);
  const end = useRef<HTMLLIElement>(null);
  const last = c.uploads.length > 0 ? c.uploads[c.uploads.length - 1] : undefined;
  const uploadLines = [
    ...c.history.filter((h) => h.role === "upload").map((h) => h.text),
    ...c.uploads.map((u) => u.filename + ", " + u.pages + "쪽"),
  ];
  const empty = !c.loading && !c.loadError && c.history.length === 0 && c.turns.length === 0;
  const panelId = "files-" + sessionId;

  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [c.turns, c.history]);

  return (
    <DropZone className="chat" label={title} blocked={c.streaming} onFiles={files.offer}>
      <header className="chat-header">
        {nav}
        <h2>{title}</h2>
        <button type="button" className="button-secondary chat-files-toggle" aria-expanded={filesOpen} aria-controls={panelId} onClick={() => setFilesOpen(!filesOpen)}>
          <Icon name="doc" />
          <span>올린 파일 {uploadLines.length}</span>
        </button>
      </header>
      {filesOpen ? (
        <aside id={panelId} className="chat-files" aria-label="파일">
          {last ? <UploadSummaryView summary={last} /> : null}
          <UploadList lines={uploadLines} />
        </aside>
      ) : null}
      <div className="chat-scroll">
        <div className="chat-column">
          {c.notice ? (
            <p className="notice notice-warn" role="alert">
              {c.notice}
            </p>
          ) : null}
          {c.loading ? (
            <p className="muted loading" role="status">
              대화 기록을 불러오는 중
            </p>
          ) : null}
          <ErrorNote message={c.loadError} />
          {c.loadError ? (
            <button type="button" className="button-secondary" onClick={c.reload}>
              다시 불러오기
            </button>
          ) : null}
          {empty ? (
            <div className="chat-empty">
              <p className="chat-empty-title">질문을 적거나 파일을 끌어다 놓으세요.</p>
              <p className="muted">올릴 수 있는 파일: PDF, DOCX, HWPX</p>
            </div>
          ) : null}
          <ol className="messages">
            <MessageList items={c.history} />
            {c.turns.map((t) => (
              <LiveTurn key={t.id} turn={t} />
            ))}
            <li ref={end} className="messages-end" aria-hidden="true" />
          </ol>
        </div>
      </div>
      <div className="composer-dock">
        <div className="chat-column">
          <AskForm
            sessionId={sessionId}
            disabled={c.streaming || files.busy || c.loading}
            waitText={c.streaming ? "답을 받는 중" : files.busy ? "파일을 올리는 중" : "보내기"}
            onAsk={c.submit}
            above={
              <>
                <FileChips items={files.items} onDismiss={files.dismiss} />
                {files.notice ? (
                  <p className="notice notice-warn composer-notice" role="alert">
                    {files.notice}
                  </p>
                ) : null}
              </>
            }
            tools={<UploadForm sessionId={sessionId} disabled={c.streaming} busy={files.busy} onFiles={files.offer} />}
          />
        </div>
      </div>
    </DropZone>
  );
}
