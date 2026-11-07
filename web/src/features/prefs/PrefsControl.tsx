import { useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { deletePrefs } from "./api";

type Props = {
  client: ApiClient;
};

export function PrefsControl({ client }: Props) {
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    if (busy || !window.confirm("저장된 답변 방식 설정을 지울까요?")) {
      return;
    }
    setBusy(true);
    setDone(false);
    setError(null);
    try {
      await deletePrefs(client);
      setDone(true);
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="prefs">
      <button type="button" onClick={run} disabled={busy}>
        답변 방식 설정 지우기
      </button>
      {done ? (
        <span className="notice" role="status">
          지웠습니다
        </span>
      ) : null}
      <ErrorNote message={error} />
    </div>
  );
}
