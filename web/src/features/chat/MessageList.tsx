import type { ReactNode } from "react";
import { Icon } from "../../shared/ui/Icon";
import { uploadLabel } from "../upload/api";
import { VerdictBlock } from "../verdict/VerdictBlock";
import { AnswerText } from "./AnswerText";
import type { HistoryItem } from "./history";

type Props = {
  items: HistoryItem[];
};

export function Question({ text }: { text: string }) {
  return (
    <div className="bubble-user">
      <h3 className="who visually-hidden">질문</h3>
      <p className="question-text">{text}</p>
    </div>
  );
}

export function AssistantRow({ children }: { children: ReactNode }) {
  return (
    <div className="assistant-row">
      <span className="assistant-mark" aria-hidden="true">
        답
      </span>
      <div className="assistant-body">{children}</div>
    </div>
  );
}

function Item({ item }: { item: HistoryItem }) {
  switch (item.role) {
    case "user":
      return <Question text={item.text} />;
    case "assistant":
      return (
        <AssistantRow>
          <h3 className="who">답변</h3>
          <AnswerText text={item.text} />
        </AssistantRow>
      );
    case "verdict":
      return (
        <AssistantRow>
          <VerdictBlock text={item.text} />
        </AssistantRow>
      );
    case "upload":
      return (
        <p className="upload-line">
          <Icon name="doc" />
          <span>파일을 올렸습니다: {uploadLabel(item.text)}</span>
        </p>
      );
  }
}

export function MessageList({ items }: Props) {
  return (
    <>
      {items.map((item, i) => (
        <li key={i} className={"message message-" + item.role}>
          <Item item={item} />
        </li>
      ))}
    </>
  );
}
