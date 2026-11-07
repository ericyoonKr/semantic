import { useState, type FormEvent, type KeyboardEvent } from "react";

type Props = {
  sessionId: string;
  disabled: boolean;
  onAsk: (question: string) => void;
};

export function AskForm({ sessionId, disabled, onAsk }: Props) {
  const [question, setQuestion] = useState("");

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
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      send();
    }
  }

  const id = "ask-" + sessionId;
  const hint = id + "-hint";
  return (
    <form className="ask-form" onSubmit={submit}>
      <label htmlFor={id}>질문</label>
      <textarea id={id} value={question} onChange={(e) => setQuestion(e.target.value)} onKeyDown={keyDown} rows={3} required aria-describedby={hint} />
      <p id={hint} className="muted hint">
        Ctrl+Enter로 보낼 수 있습니다.
      </p>
      <button type="submit" disabled={disabled || question.trim() === ""}>
        {disabled ? "답을 받는 중" : "보내기"}
      </button>
    </form>
  );
}
