import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { ApiClient } from "../../shared/api/client";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { Icon } from "../../shared/ui/Icon";
import { DropZone } from "../upload/DropZone";
import { FileChips } from "../upload/FileChips";
import { FilePageView } from "../upload/FilePageView";
import { UploadForm } from "../upload/UploadForm";
import { UploadedFiles } from "../upload/UploadedFiles";
import { UploadSummaryView } from "../upload/UploadSummaryView";
import { useUploadQueue } from "../upload/useUploadQueue";
import { SourceDetail } from "./AnswerText";
import { AskForm } from "./AskForm";
import type { Source } from "./events";
import { LiveTurn } from "./LiveTurn";
import { MessageList } from "./MessageList";
import { type DocRef, SourceViewContext, type SourceView } from "./sourceView";
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
  const [shown, setShown] = useState<Source | null>(null);
  const [shownDoc, setShownDoc] = useState<DocRef | null>(null);
  const [paneOpen, setPaneOpen] = useState(false);
  const end = useRef<HTMLLIElement>(null);
  const last = c.uploads.length > 0 ? c.uploads[c.uploads.length - 1] : undefined;
  const uploadLines = [
    ...c.history.filter((h) => h.role === "upload").map((h) => h.text),
    ...c.uploads.map((u) => u.filename + ", " + u.pages + "쪽"),
  ];
  const empty = !c.loading && !c.loadError && c.history.length === 0 && c.turns.length === 0;
  const paneId = "pane-" + sessionId;

  const view = useMemo<SourceView>(
    () => ({
      shown,
      open: (s: Source) => {
        setShownDoc(null);
        setShown(s);
        setPaneOpen(true);
      },
      openDoc: (ref: DocRef) => {
        setShown(null);
        setShownDoc(ref);
        setPaneOpen(true);
      },
    }),
    [shown],
  );
  const closeSource = useCallback(() => {
    setShown(null);
    setShownDoc(null);
    setPaneOpen(false);
  }, []);

  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [c.turns, c.history]);

  return (
    <SourceViewContext.Provider value={view}>
      <div className={paneOpen ? "conversation pane-open" : "conversation"}>
        <DropZone className="chat" label={title} blocked={c.streaming} onFiles={files.offer}>
          <header className="chat-header">
            {nav}
            <h2 title={title}>{title}</h2>
            <button type="button" className="button-ghost pane-toggle" aria-expanded={paneOpen} aria-controls={paneId} onClick={() => setPaneOpen(!paneOpen)}>
              <Icon name="panel" />
              <span>근거·파일 {uploadLines.length > 0 ? uploadLines.length : ""}</span>
            </button>
          </header>
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
                  <span className="chat-empty-mark" aria-hidden="true">
                    <Icon name="scale" />
                  </span>
                  <p className="chat-empty-title">무엇이 궁금하신가요?</p>
                  <p className="muted">법령·판례를 먼저 찾아 읽은 뒤, 읽은 근거만으로 답합니다. 처분서·계약서 같은 파일(PDF, DOCX, HWPX)을 끌어다 놓아도 됩니다.</p>
                </div>
              ) : null}
              <ol className="messages">
                <MessageList items={c.history} />
                {c.turns.map((t) => (
                  <LiveTurn key={t.id} turn={t} />
                ))}
                {c.canRetry ? (
                  <li className="message retry-row">
                    <button type="button" className="button-ghost retry-button" onClick={c.retry} disabled={files.busy}>
                      <Icon name="redo" />
                      <span>다시 답변하기</span>
                    </button>
                  </li>
                ) : null}
                <li ref={end} className="messages-end" aria-hidden="true" />
              </ol>
            </div>
          </div>
          <div className="composer-dock">
            <div className="chat-column">
              <AskForm
                sessionId={sessionId}
                disabled={files.busy || c.loading}
                waitText={files.busy ? "파일을 올리는 중" : "보내기"}
                onAsk={c.submit}
                above={
                  <>
                    <FileChips items={files.items} onDismiss={files.dismiss} onCancel={files.cancel} />
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
        {paneOpen ? <button type="button" className="pane-backdrop" aria-label="근거·파일 창 닫기" tabIndex={-1} onClick={closeSource} /> : null}
        <aside id={paneId} className="pane" aria-label="근거 원문과 올린 파일">
          {shown ? (
            <SourceDetail source={shown} close={closeSource} />
          ) : shownDoc ? (
            <FilePageView client={client} sessionId={sessionId} target={shownDoc} close={closeSource} />
          ) : (
            <div className="pane-files">
              <header className="pane-head">
                <h3>올린 파일 {uploadLines.length}</h3>
                <button type="button" className="icon-button pane-close" aria-label="근거·파일 창 닫기" onClick={closeSource}>
                  <Icon name="close" />
                </button>
              </header>
              {last ? <UploadSummaryView summary={last} /> : null}
              <UploadedFiles client={client} sessionId={sessionId} version={uploadLines.length} />
              <p className="pane-hint">답 속 조문·판례나 아래 근거 칩을 누르면 원문이 이 자리에 열리고, 답에 쓴 부분에 표시가 됩니다.</p>
            </div>
          )}
        </aside>
      </div>
    </SourceViewContext.Provider>
  );
}
