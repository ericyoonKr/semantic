import { Fold } from "../../shared/ui/Fold";

export const LONG_ANSWER_CHARS = 1500;

type Props = {
  text: string;
  streaming?: boolean;
};

export function AnswerText({ text, streaming = false }: Props) {
  return (
    <Fold long={!streaming && text.length > LONG_ANSWER_CHARS}>
      <p className="answer-text">{text}</p>
    </Fold>
  );
}
