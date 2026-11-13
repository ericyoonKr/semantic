import { useState, type FormEvent, type KeyboardEvent, type ReactNode } from "react";
import { Icon } from "../../shared/ui/Icon";
import { readDraft, writeDraft } from "./drafts";

type Props = {
  sessionId: string;
  disabled: boolean;
  waitText: string;
  onAsk: (question: string) => void;
  above?: ReactNode;
  tools?: ReactNode;
};

export function sendsOnKey(key: string, shift: boolean, composing: boolean): boolean {
  return key === "Enter" && !shift && !composing;
}

export function AskForm({ sessionId, disabled, waitText, onAsk, above, tools }: Props) {
  const [question, setQuestionState] = useState(() => readDraft(sessionId));

  function setQuestion(text: string) {
    setQuestionState(text);
    writeDraft(sessionId, text);
  }

  function send() {
    const q = question.trim();
    if (!q || disabled) {
      return;
    }
    onAsk(q);
    setQuestion("");
  }

  function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    send();
  }

  function keyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (sendsOnKey(e.key, e.shiftKey, e.nativeEvent.isComposing)) {
      e.preventDefault();
      send();
    }
  }

  const id = "ask-" + sessionId;
  const hint = id + "-hint";
  return (
    <form className="composer" onSubmit={submit}>
      {above}
      <label htmlFor={id} className="visually-hidden">
        질문
      </label>
      <textarea
        id={id}
        value={question}
        onChange={(e) => setQuestion(e.target.value)}
        onKeyDown={keyDown}
        rows={3}
        required
        aria-describedby={hint}
        placeholder="질문을 적어 주세요"
      />
      <div className="composer-actions">
        {tools}
        <p id={hint} className="composer-hint">
          Enter로 보내고, Shift+Enter로 줄을 바꿉니다.
        </p>
        <button type="submit" className="button-primary" disabled={disabled || question.trim() === ""}>
          <span>{disabled ? waitText : "보내기"}</span>
          <Icon name="send" />
        </button>
      </div>
    </form>
  );
}
