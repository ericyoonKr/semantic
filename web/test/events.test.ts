import { describe, expect, it } from "vitest";
import { STREAM_ERROR_TEXT, toChatEvent } from "../src/features/chat/events";

const ev = (event: string, data: string) => toChatEvent({ event, data });

describe("toChatEvent", () => {
  it("maps each event the gateway sends", () => {
    expect(ev("Status", '{"text":"판단하는 중"}')).toEqual({ kind: "status", text: "판단하는 중" });
    expect(ev("Token", '{"text":"답"}')).toEqual({ kind: "token", text: "답" });
    expect(ev("VerdictReady", '{"text":"판정 문장"}')).toEqual({ kind: "verdict", text: "판정 문장" });
    expect(ev("NeededFacts", '{"asks":["언제 처분을 받았나요?"]}')).toEqual({ kind: "needed", asks: ["언제 처분을 받았나요?"] });
    expect(ev("Warning", '{"message":"일부 자료를 찾지 못했습니다"}')).toEqual({ kind: "warning", message: "일부 자료를 찾지 못했습니다" });
    expect(ev("Error", '{"message":"답을 만들지 못했습니다","request_id":"abc"}')).toEqual({ kind: "error", message: "답을 만들지 못했습니다", requestId: "abc" });
    expect(ev("Done", "{}")).toEqual({ kind: "done" });
  });

  it("keeps only the known field and drops unknown events", () => {
    expect(ev("VerdictReady", '{"text":"판정","node_ids":["R1"]}')).toEqual({ kind: "verdict", text: "판정" });
    expect(ev("Usage", '{"tokens":5}')).toBeNull();
    expect(ev("Debug", '{"text":"x"}')).toBeNull();
    expect(ev("message", '{"text":"x"}')).toBeNull();
  });

  it("drops frames without usable content", () => {
    expect(ev("Token", '{"text":""}')).toBeNull();
    expect(ev("Token", "not json")).toBeNull();
    expect(ev("Status", "{}")).toBeNull();
    expect(ev("NeededFacts", '{"asks":[1,null,""]}')).toBeNull();
    expect(ev("NeededFacts", '{"asks":["질문", 3]}')).toEqual({ kind: "needed", asks: ["질문"] });
  });

  it("turns a broken error frame into the general message", () => {
    expect(ev("Error", "{}")).toEqual({ kind: "error", message: STREAM_ERROR_TEXT, requestId: "" });
  });
});
