import { useEffect, useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { RichBlocks } from "../chat/AnswerText";
import { listFiles, loadFileText, toPages, type FileText, type StoredFile } from "./files";

type Props = {
  client: ApiClient;
  sessionId: string;
  version: number;
};

export function UploadedFiles({ client, sessionId, version }: Props) {
  const [files, setFiles] = useState<StoredFile[]>([]);
  const [open, setOpen] = useState<FileText | null>(null);
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
    try {
      setOpen(await loadFileText(client, sessionId, f.fileId));
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
      {open ? (
        <div className="file-view" aria-label={open.filename + " 원문"}>
          <header className="source-head">
            <strong>{open.filename}</strong>
            <span className="source-note">{open.pages > 0 ? open.pages + "쪽 · " : ""}추출한 글</span>
            <button type="button" className="source-close" onClick={() => setOpen(null)} aria-label="닫기">
              ×
            </button>
          </header>
          <div className="file-body">
            {toPages(open.text).map((p, i) => (
              <section key={i} className="file-page">
                {p.page !== null ? <p className="file-page-mark">{p.page}쪽</p> : null}
                <div className="answer-text file-page-text">
                  <RichBlocks text={p.text} />
                </div>
              </section>
            ))}
          </div>
        </div>
      ) : null}
    </section>
  );
}
