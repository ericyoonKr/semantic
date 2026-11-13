import { ErrorNote } from "../../shared/ui/ErrorNote";
import { NeededFacts } from "../verdict/NeededFacts";
import { VerdictBlock } from "../verdict/VerdictBlock";
import { AnswerText } from "./AnswerText";
import { AssistantRow, Question } from "./MessageList";
import type { Turn } from "./turn";

type Props = {
  turn: Turn;
};

export function LiveTurn({ turn }: Props) {
  const streaming = turn.state === "streaming";
  return (
    <li className="message turn" aria-busy={streaming}>
      <Question text={turn.question} />
      <AssistantRow>
        {turn.warnings.length > 0 ? (
          <ul className="warnings" aria-label="알림">
            {turn.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        ) : null}
        {turn.verdicts.map((v, i) => (
          <VerdictBlock key={i} text={v} />
        ))}
        {turn.answer !== "" ? (
          <>
            <h3 className="who">답변</h3>
            <AnswerText text={turn.answer} streaming={streaming} sources={turn.sources} />
          </>
        ) : null}
        <NeededFacts asks={turn.asks} />
        {streaming ? (
          <p className="status" role="status">
            <span className="spinner" aria-hidden="true" />
            <span>{turn.status ?? "질문을 보냈습니다"}</span>
          </p>
        ) : null}
        <ErrorNote message={turn.error?.message ?? null} requestId={turn.error?.requestId} />
      </AssistantRow>
    </li>
  );
}
