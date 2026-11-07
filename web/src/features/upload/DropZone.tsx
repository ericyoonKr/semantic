import { useRef, useState, type DragEvent, type ReactNode } from "react";
import { Icon } from "../../shared/ui/Icon";
import { BLOCKED_TEXT, hasFiles } from "./dropQueue";

type Props = {
  blocked: boolean;
  label: string;
  className: string;
  onFiles: (files: File[]) => void;
  children: ReactNode;
};

export function DropZone({ blocked, label, className, onFiles, children }: Props) {
  const [active, setActive] = useState(false);
  const depth = useRef(0);

  function enter(e: DragEvent<HTMLElement>) {
    if (!hasFiles(e.dataTransfer.types)) {
      return;
    }
    e.preventDefault();
    depth.current += 1;
    setActive(true);
  }

  function over(e: DragEvent<HTMLElement>) {
    if (!hasFiles(e.dataTransfer.types)) {
      return;
    }
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }

  function leave(e: DragEvent<HTMLElement>) {
    if (!hasFiles(e.dataTransfer.types)) {
      return;
    }
    depth.current = Math.max(0, depth.current - 1);
    if (depth.current === 0) {
      setActive(false);
    }
  }

  function drop(e: DragEvent<HTMLElement>) {
    if (!hasFiles(e.dataTransfer.types)) {
      return;
    }
    e.preventDefault();
    depth.current = 0;
    setActive(false);
    onFiles(Array.from(e.dataTransfer.files));
  }

  return (
    <section className={active ? className + " is-dragging" : className} aria-label={label} onDragEnter={enter} onDragOver={over} onDragLeave={leave} onDrop={drop}>
      {children}
      {active ? (
        <div className={blocked ? "drop-overlay is-blocked" : "drop-overlay"} aria-hidden="true">
          <div className="drop-overlay-card">
            <span className="drop-overlay-icon">
              <Icon name="tray" />
            </span>
            {blocked ? (
              <strong>{BLOCKED_TEXT}</strong>
            ) : (
              <>
                <strong>여기에 놓으면 파일을 올립니다</strong>
                <span>PDF · DOCX · HWPX, 여러 개를 한 번에 놓을 수 있습니다</span>
              </>
            )}
          </div>
        </div>
      ) : null}
    </section>
  );
}
