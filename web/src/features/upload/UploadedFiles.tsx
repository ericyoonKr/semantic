import { type RefObject, useEffect, useRef, useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { RichBlocks } from "../chat/AnswerText";
import { listFiles, loadFileText, originalUrl, showsOriginal, toPages, type FileText, type StoredFile } from "./files";

type Props = {
  client: ApiClient;
  sessionId: string;
  version: number;
};

type ViewProps = {
  sessionId: string;
  file: StoredFile;
  text: FileText | null;
  page: number | null;
  close: () => void;
};

export function FileView({ sessionId, file, text, page, close }: ViewProps) {
  const target = useRef<HTMLElement>(null);
  useEffect(() => {
    target.current?.scrollIntoView?.({ block: "start" });
  }, [file, text, page]);
  const original = showsOriginal(file);
  const src = originalUrl(sessionId, file.fileId) + (page !== null ? "#page=" + page : "");
  return (
    <div className="file-view" aria-label={file.filename + " 원문"}>
      <header className="source-head">
        <strong>{file.filename}</strong>
        <span className="source-note">
          {file.pages > 0 ? file.pages + "쪽 · " : ""}
          {original ? "원본" : "추출한 글"}
          {page !== null ? " · " + page + "쪽" : ""}
        </span>
        <a className="file-download" href={originalUrl(sessionId, file.fileId)} download={file.filename}>
          원본 내려받기
        </a>
        <button type="button" className="source-close" onClick={close} aria-label="닫기">
          ×
        </button>
      </header>
      {original ? (
        <iframe className="file-original" src={src} title={file.filename + " 원본"} />
      ) : (
        <div className="file-body">
          {toPages(text?.text ?? "").map((p, i) => {
            const hit = page !== null && p.page === page;
            return (
              <section key={i} ref={hit ? (target as RefObject<HTMLElement>) : undefined} className={hit ? "file-page is-target" : "file-page"}>
                {p.page !== null ? <p className="file-page-mark">{p.page}쪽</p> : null}
                <div className="answer-text file-page-text">
                  <RichBlocks text={p.text} />
                </div>
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function UploadedFiles({ client, sessionId, version }: Props) {
  const [files, setFiles] = useState<StoredFile[]>([]);
  const [open, setOpen] = useState<StoredFile | null>(null);
  const [text, setText] = useState<FileText | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    listFiles(client, sessionId)
      .then((list) => alive && setFiles(list))
      .catch((err: unknown) => alive && setError(messageOf(err)));
    return () => {
      alive = false;
    };
  }, [client, sessionId, version]);

  async function show(f: StoredFile) {
    setError(null);
    setText(null);
    setOpen(f);
    if (showsOriginal(f)) {
      return;
    }
    try {
      setText(await loadFileText(client, sessionId, f.fileId));
    } catch (err) {
      setError(messageOf(err));
    }
  }

  return (
    <section className="upload-list" aria-label="올린 파일">
      <h3>올린 파일</h3>
      {files.length === 0 ? <p className="muted">아직 올린 파일이 없습니다.</p> : null}
      <ul>
        {files.map((f) => (
          <li key={f.fileId} className="uploaded-file">
            <span>
              {f.filename}
              {f.pages > 0 ? " · " + f.pages + "쪽" : ""}
            </span>
            <button type="button" className="button-secondary file-open" onClick={() => void show(f)}>
              원문 보기
            </button>
          </li>
        ))}
      </ul>
      <ErrorNote message={error} />
      {open ? <FileView sessionId={sessionId} file={open} text={text} page={null} close={() => setOpen(null)} /> : null}
    </section>
  );
}
