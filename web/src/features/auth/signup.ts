export type AuthMode = "login" | "signup";

export type AuthState = {
  mode: AuthMode;
  busy: boolean;
  error: string | null;
  notice: string | null;
};

export type AuthEvent =
  | { kind: "switch"; to: AuthMode }
  | { kind: "submit" }
  | { kind: "signedUp" }
  | { kind: "failed"; message: string }
  | { kind: "settled" };

export const SIGNED_UP_TEXT = "신청했습니다. 마스터가 승인하면 로그인할 수 있습니다.";
export const LOGIN_ID_RULE = /^[a-z0-9_.-]{4,32}$/;
export const LOGIN_ID_HINT = "4~32자의 영문 소문자, 숫자, _ . -";
export const PASSWORD_MIN_CHARS = 8;
export const NAME_MAX_CHARS = 40;

export function initialAuth(): AuthState {
  return { mode: "login", busy: false, error: null, notice: null };
}

export function authReducer(state: AuthState, event: AuthEvent): AuthState {
  switch (event.kind) {
    case "switch":
      return { mode: event.to, busy: false, error: null, notice: null };
    case "submit":
      return { ...state, busy: true, error: null };
    case "signedUp":
      return { mode: "login", busy: false, error: null, notice: SIGNED_UP_TEXT };
    case "failed":
      return { ...state, busy: false, error: event.message };
    case "settled":
      return { ...state, busy: false };
  }
}

export function checkSignup(loginId: string, password: string, name: string): string | null {
  if (!LOGIN_ID_RULE.test(loginId)) {
    return "아이디는 " + LOGIN_ID_HINT + " 만 쓸 수 있습니다";
  }
  if ([...password].length < PASSWORD_MIN_CHARS) {
    return "비밀번호는 8자 이상이어야 합니다";
  }
  const length = [...name.trim()].length;
  if (length < 1 || length > NAME_MAX_CHARS) {
    return "이름은 1~40자여야 합니다";
  }
  return null;
}
