import { useEffect, useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { Icon } from "../../shared/ui/Icon";
import type { DocRef } from "../chat/sourceView";
import { findFile, listFiles, loadFileText, showsOriginal, type FileText, type StoredFile } from "./files";
import { FileView } from "./UploadedFiles";

type Props = {
  client: ApiClient;
  sessionId: string;
  target: DocRef;
  close: () => void;
};

export function FilePageView({ client, sessionId, target, close }: Props) {
  const [file, setFile] = useState<StoredFile | null>(null);
  const [text, setText] = useState<FileText | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);

  useEffect(() => {
    let alive = true;
    setFile(null);
    setText(null);
    setError(null);
    setMissing(false);
    listFiles(client, sessionId)
      .then(async (files) => {
        const found = findFile(files, target.filename);
        if (!alive) {
          return;
        }
        if (!found) {
          setMissing(true);
          return;
        }
        setFile(found);
        if (!showsOriginal(found)) {
          const loaded = await loadFileText(client, sessionId, found.fileId);
          if (alive) {
            setText(loaded);
          }
        }
      })
      .catch((err: unknown) => {
        if (alive) {
          setError(messageOf(err));
        }
      });
    return () => {
      alive = false;
    };
  }, [client, sessionId, target]);

  if (file) {
    return <FileView sessionId={sessionId} file={file} text={text} page={target.page} close={close} />;
  }
  return (
    <div className="file-view" aria-label={target.filename + " 원문"}>
      <header className="source-head">
        <strong>{target.filename}</strong>
        <button type="button" className="icon-button source-close" onClick={close} aria-label="닫기">
          <Icon name="close" />
        </button>
      </header>
      <ErrorNote message={error} />
      {missing ? <p className="muted">올린 파일 가운데 이 이름의 파일이 없습니다.</p> : null}
      {!missing && !error ? (
        <p className="muted loading" role="status">
          원문을 불러오는 중
        </p>
      ) : null}
    </div>
  );
}
