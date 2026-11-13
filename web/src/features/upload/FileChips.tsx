import { Icon } from "../../shared/ui/Icon";
import type { QueueItem } from "./dropQueue";

type Props = {
  items: QueueItem[];
  onDismiss: (id: number) => void;
  onCancel: (id: number) => void;
};

function stateText(item: QueueItem): string {
  switch (item.state) {
    case "waiting":
      return "차례를 기다리는 중";
    case "uploading":
      return "올리는 중";
    case "done":
      return item.pages !== null && item.pages > 0 ? `올렸습니다 · ${item.pages}쪽` : "올렸습니다";
    case "canceled":
      return "올리기를 멈췄습니다";
    case "failed":
    case "rejected":
      return item.message ?? "올리지 못했습니다";
  }
}

export function FileChips({ items, onDismiss, onCancel }: Props) {
  if (items.length === 0) {
    return null;
  }
  return (
    <ul className="file-chips" aria-label="올리는 파일" aria-live="polite">
      {items.map((item) => {
        const finished = item.state !== "waiting" && item.state !== "uploading";
        return (
          <li key={item.id} className={`file-chip is-${item.state}`}>
            <span className="file-chip-icon">
              <Icon name="doc" />
            </span>
            <span className="file-chip-copy">
              <strong>{item.name}</strong>
              <small role={item.state === "failed" || item.state === "rejected" ? "alert" : undefined}>{stateText(item)}</small>
            </span>
            {item.state === "uploading" ? <span className="file-chip-bar" aria-hidden="true" /> : null}
            {finished ? (
              <button type="button" className="icon-button" aria-label={item.name + " 목록에서 지우기"} onClick={() => onDismiss(item.id)}>
                <Icon name="close" />
              </button>
            ) : (
              <button type="button" className="file-chip-cancel" aria-label={item.name + " 올리기 멈추기"} onClick={() => onCancel(item.id)}>
                멈추기
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}
