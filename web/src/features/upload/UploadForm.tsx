import { useRef, type ChangeEvent } from "react";
import { Icon } from "../../shared/ui/Icon";
import { ACCEPT_ATTR } from "./dropQueue";

type Props = {
  sessionId: string;
  disabled: boolean;
  busy: boolean;
  onFiles: (files: File[]) => void;
};

export function UploadForm({ sessionId, disabled, busy, onFiles }: Props) {
  const input = useRef<HTMLInputElement>(null);
  const id = `upload-${sessionId}`;

  function picked(e: ChangeEvent<HTMLInputElement>) {
    const files = Array.from(e.target.files ?? []);
    e.target.value = "";
    if (files.length > 0) {
      onFiles(files);
    }
  }

  return (
    <div className="attach">
      <input id={id} ref={input} className="visually-hidden" type="file" name="file" accept={ACCEPT_ATTR} multiple disabled={disabled} onChange={picked} tabIndex={-1} />
      <button type="button" className="button-secondary" onClick={() => input.current?.click()} disabled={disabled} aria-describedby={id + "-types"}>
        <Icon name="attach" />
        <span>{busy ? "올리는 중" : "파일 올리기"}</span>
      </button>
      <span id={id + "-types"} className="attach-types">
        PDF, DOCX, HWPX
      </span>
    </div>
  );
}
