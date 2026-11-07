import { uploadLabel } from "../upload/api";
import { VerdictBlock } from "../verdict/VerdictBlock";
import { AnswerText } from "./AnswerText";
import type { HistoryItem } from "./history";

type Props = {
  items: HistoryItem[];
};

function Item({ item }: { item: HistoryItem }) {
  switch (item.role) {
    case "user":
      return (
        <>
          <h3 className="who">질문</h3>
          <p className="question-text">{item.text}</p>
        </>
      );
    case "assistant":
      return (
        <>
          <h3 className="who">답변</h3>
          <AnswerText text={item.text} />
        </>
      );
    case "verdict":
      return <VerdictBlock text={item.text} />;
    case "upload":
      return <p className="upload-line">파일을 올렸습니다: {uploadLabel(item.text)}</p>;
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
