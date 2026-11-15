import { useReducer, useState, type FormEvent } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { Icon } from "../../shared/ui/Icon";
import { login, signup } from "./api";
import { LOGIN_ID_HINT, NAME_MAX_CHARS, PASSWORD_MIN_CHARS, authReducer, checkSignup, initialAuth } from "./signup";
import "./auth.css";

type Props = {
  client: ApiClient;
  notice: string | null;
  onSignedIn: () => void;
};

export function LoginForm({ client, notice, onSignedIn }: Props) {
  const [loginId, setLoginId] = useState("");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [state, dispatch] = useReducer(authReducer, undefined, initialAuth);
  const signingUp = state.mode === "signup";
  const shown = state.notice ?? notice;

  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (state.busy) {
      return;
    }
    const id = loginId.trim();
    if (signingUp) {
      const problem = checkSignup(id, password, name);
      if (problem !== null) {
        dispatch({ kind: "failed", message: problem });
        return;
      }
    }
    dispatch({ kind: "submit" });
    try {
      if (signingUp) {
        await signup(client, id, password, name.trim());
        setPassword("");
        dispatch({ kind: "signedUp" });
      } else {
        await login(client, id, password);
        setPassword("");
        dispatch({ kind: "settled" });
        onSignedIn();
      }
    } catch (err) {
      dispatch({ kind: "failed", message: messageOf(err) });
    }
  }

  function switchMode() {
    dispatch({ kind: "switch", to: signingUp ? "login" : "signup" });
  }

  const title = signingUp ? "가입 신청" : "로그인";
  const submitText = signingUp ? (state.busy ? "신청하는 중" : "가입 신청") : state.busy ? "로그인하는 중" : "로그인";

  return (
    <main className="login">
      <div className="login-card">
        <p className="login-product">
          <span className="brand-mark" aria-hidden="true">
            <Icon name="scale" />
          </span>
          <span>법률 질의</span>
        </p>
        <h1>{title}</h1>
        {shown && !state.error ? (
          <p className="notice" role="status">
            {shown}
          </p>
        ) : null}
        <form onSubmit={submit} className="login-form" aria-label={title}>
          <label htmlFor="login-id">아이디</label>
          <input
            id="login-id"
            name="username"
            autoComplete="username"
            value={loginId}
            onChange={(e) => setLoginId(e.target.value)}
            aria-describedby={signingUp ? "login-id-hint" : undefined}
            required
          />
          {signingUp ? (
            <p id="login-id-hint" className="auth-hint">
              {LOGIN_ID_HINT}
            </p>
          ) : null}
          <label htmlFor="login-password">비밀번호</label>
          <input
            id="login-password"
            name="password"
            type="password"
            autoComplete={signingUp ? "new-password" : "current-password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            aria-describedby={signingUp ? "login-password-hint" : undefined}
            required
          />
          {signingUp ? (
            <>
              <p id="login-password-hint" className="auth-hint">
                {PASSWORD_MIN_CHARS}자 이상
              </p>
              <label htmlFor="signup-name">이름</label>
              <input
                id="signup-name"
                name="name"
                autoComplete="name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                maxLength={NAME_MAX_CHARS}
                required
              />
            </>
          ) : null}
          <button type="submit" className="button-primary" disabled={state.busy}>
            {submitText}
          </button>
          <ErrorNote message={state.error} />
        </form>
        <p className="auth-switch">
          <span className="muted">{signingUp ? "이미 계정이 있나요?" : "계정이 없나요?"}</span>
          <button type="button" className="link-button" onClick={switchMode} disabled={state.busy}>
            {signingUp ? "로그인으로" : "가입 신청"}
          </button>
        </p>
      </div>
    </main>
  );
}
