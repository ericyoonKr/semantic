import { describe, expect, it } from "vitest";
import { SIGNED_UP_TEXT, authReducer, checkSignup, initialAuth } from "../src/features/auth/signup";
import { confirmText, splitUsers, statusLabel } from "../src/features/auth/users";

describe("가입 신청 폼 상태", () => {
  it("가입 신청으로 바꾸면 오류가 지워지고, 신청에 성공하면 로그인 모드로 돌아가 안내 문구를 보인다", () => {
    let s = authReducer(initialAuth(), { kind: "failed", message: "아이디 또는 비밀번호가 맞지 않습니다" });
    expect(s.error).toBe("아이디 또는 비밀번호가 맞지 않습니다");
    s = authReducer(s, { kind: "switch", to: "signup" });
    expect(s).toEqual({ mode: "signup", busy: false, error: null, notice: null });
    s = authReducer(s, { kind: "submit" });
    expect(s.busy).toBe(true);
    s = authReducer(s, { kind: "signedUp" });
    expect(s).toEqual({ mode: "login", busy: false, error: null, notice: SIGNED_UP_TEXT });
    expect(SIGNED_UP_TEXT).toBe("신청했습니다. 마스터가 승인하면 로그인할 수 있습니다.");
    s = authReducer(s, { kind: "switch", to: "signup" });
    expect(s.notice).toBeNull();
  });

  it("승인 대기 403 문구는 그대로 오류로 남고 보내는 중 표시는 풀린다", () => {
    const s = authReducer(authReducer(initialAuth(), { kind: "submit" }), { kind: "failed", message: "승인 대기 중입니다" });
    expect(s).toEqual({ mode: "login", busy: false, error: "승인 대기 중입니다", notice: null });
  });

  it("보내기 전에 아이디·비밀번호·이름 규칙을 검사한다", () => {
    expect(checkSignup("abc", "eight888", "이름")).toMatch(/아이디/);
    expect(checkSignup("Upper1", "eight888", "이름")).toMatch(/아이디/);
    expect(checkSignup("a".repeat(33), "eight888", "이름")).toMatch(/아이디/);
    expect(checkSignup("good.id_1", "seven77", "이름")).toMatch(/비밀번호/);
    expect(checkSignup("good.id_1", "eight888", "   ")).toMatch(/이름/);
    expect(checkSignup("good.id_1", "eight888", "가".repeat(41))).toMatch(/이름/);
    expect(checkSignup("good.id_1", "eight888", " 홍길동 ")).toBeNull();
  });
});

describe("가입 승인 목록", () => {
  it("대기 중을 따로 모으고 상태와 확인 문구를 한국어로 보인다", () => {
    const users = [
      { loginId: "older", name: "가", status: "active" as const, requestedAt: "2026-11-15T01:00:00Z" },
      { loginId: "newer", name: "나", status: "pending" as const, requestedAt: "2026-11-15T02:00:00Z" },
      { loginId: "off", name: "다", status: "disabled" as const, requestedAt: "2026-11-15T03:00:00Z" },
    ];
    const { pending, others } = splitUsers(users);
    expect(pending.map((u) => u.loginId)).toEqual(["newer"]);
    expect(others.map((u) => u.loginId)).toEqual(["older", "off"]);
    expect((["pending", "active", "disabled"] as const).map(statusLabel)).toEqual(["대기", "활성", "꺼짐"]);
    expect(confirmText("approve", users[1]!)).toBeNull();
    expect(confirmText("reject", users[1]!)).toContain("newer");
    expect(confirmText("disable", users[0]!)).toContain("older");
  });
});
