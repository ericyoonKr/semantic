import type { ChatEvent } from "./events";

export const BROKEN_TEXT = "연결이 끊겼습니다";

export type TurnState = "streaming" | "done" | "error" | "broken";

export type Turn = {
  id: number;
  question: string;
  status: string | null;
  answer: string;
  verdicts: string[];
  asks: string[];
  warnings: string[];
  error: { message: string; requestId: string } | null;
  state: TurnState;
};

export function startTurn(id: number, question: string): Turn {
  return { id, question, status: null, answer: "", verdicts: [], asks: [], warnings: [], error: null, state: "streaming" };
}

function addOnce(list: string[], item: string): string[] {
  return list.includes(item) ? list : [...list, item];
}

export function applyEvent(turn: Turn, event: ChatEvent): Turn {
  if (turn.state !== "streaming") {
    return turn;
  }
  switch (event.kind) {
    case "status":
      return { ...turn, status: event.text };
    case "token":
      return { ...turn, answer: turn.answer + event.text };
    case "verdict":
      return { ...turn, verdicts: [...turn.verdicts, event.text] };
    case "needed":
      return { ...turn, asks: event.asks.reduce(addOnce, turn.asks) };
    case "warning":
      return { ...turn, warnings: addOnce(turn.warnings, event.message) };
    case "error":
      return { ...turn, status: null, state: "error", error: { message: event.message, requestId: event.requestId } };
    case "done":
      return { ...turn, status: null, state: "done" };
  }
}

export function breakTurn(turn: Turn): Turn {
  if (turn.state !== "streaming") {
    return turn;
  }
  return { ...turn, status: null, state: "broken", error: { message: BROKEN_TEXT, requestId: "" } };
}
