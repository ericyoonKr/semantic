import { useState, type FormEvent } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { Icon } from "../../shared/ui/Icon";
import { login } from "./api";

type Props = {
  client: ApiClient;
  notice: string | null;
  onSignedIn: () => void;
};

export function LoginForm({ client, notice, onSignedIn }: Props) {
  const [loginId, setLoginId] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (busy) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await login(client, loginId.trim(), password);
      setPassword("");
      onSignedIn();
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="login">
      <div className="login-card">
        <p className="login-product">
          <span className="brand-mark" aria-hidden="true">
            <Icon name="scale" />
          </span>
          <span>법률 질의</span>
        </p>
        <h1>로그인</h1>
        {notice && !error ? (
          <p className="notice" role="status">
            {notice}
          </p>
        ) : null}
        <form onSubmit={submit} className="login-form">
          <label htmlFor="login-id">아이디</label>
          <input id="login-id" name="username" autoComplete="username" value={loginId} onChange={(e) => setLoginId(e.target.value)} required />
          <label htmlFor="login-password">비밀번호</label>
          <input
            id="login-password"
            name="password"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
          <button type="submit" className="button-primary" disabled={busy}>
            {busy ? "로그인하는 중" : "로그인"}
          </button>
          <ErrorNote message={error} />
        </form>
      </div>
    </main>
  );
}
