import { useRef, useState, type FormEvent } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { uploadFile, type UploadSummary } from "./api";

type Props = {
  client: ApiClient;
  sessionId: string;
  disabled: boolean;
  onBusy: (busy: boolean) => void;
  onUploaded: (summary: UploadSummary) => void;
};

export function UploadForm({ client, sessionId, disabled, onBusy, onUploaded }: Props) {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const file = input.current?.files?.[0];
    if (!file || busy) {
      return;
    }
    setBusy(true);
    onBusy(true);
    setError(null);
    try {
      const summary = await uploadFile(client, sessionId, file);
      if (input.current) {
        input.current.value = "";
      }
      onUploaded(summary);
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
      onBusy(false);
    }
  }

  return (
    <form className="upload-form" onSubmit={submit}>
      <label htmlFor={`upload-${sessionId}`}>파일 올리기 (PDF, DOCX, HWPX)</label>
      <input id={`upload-${sessionId}`} ref={input} type="file" name="file" accept=".pdf,.docx,.hwpx" disabled={disabled || busy} required />
      <button type="submit" disabled={disabled || busy}>
        {busy ? "올리는 중" : "올리기"}
      </button>
      <ErrorNote message={error} />
    </form>
  );
}
