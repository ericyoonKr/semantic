import { useCallback, useEffect, useMemo, useState } from "react";
import { logout, me } from "../features/auth/api";
import { LoginForm } from "../features/auth/LoginForm";
import { PrefsControl } from "../features/prefs/PrefsControl";
import { ApiClient } from "../shared/api/client";
import { SIGN_IN_TEXT } from "../shared/api/errors";
import { sessionStore } from "../shared/api/tokenStore";
import { Disclaimer } from "../shared/ui/Disclaimer";
import { Workspace } from "./Workspace";

type Phase = { kind: "checking" } | { kind: "signedOut"; notice: string | null } | { kind: "signedIn"; loginId: string };

const CSRF_KEY = "csrf";

export function App() {
  const client = useMemo(() => new ApiClient({ tokens: sessionStore(CSRF_KEY) }), []);
  const [phase, setPhase] = useState<Phase>({ kind: "checking" });
  const [streaming, setStreaming] = useState(false);

  const check = useCallback(() => {
    if (!client.hasCsrf()) {
      setPhase({ kind: "signedOut", notice: null });
      return;
    }
    me(client)
      .then((loginId) => setPhase({ kind: "signedIn", loginId }))
      .catch(() => setPhase({ kind: "signedOut", notice: null }));
  }, [client]);

  useEffect(() => {
    client.onUnauthorized(() => setPhase({ kind: "signedOut", notice: SIGN_IN_TEXT }));
    check();
    return () => client.onUnauthorized(undefined);
  }, [client, check]);

  async function signOut() {
    try {
      await logout(client);
    } catch {
      client.setCsrf(null);
    }
    setPhase({ kind: "signedOut", notice: "로그아웃했습니다" });
  }

  return (
    <div className="app">
      <Disclaimer />
      {phase.kind === "checking" ? (
        <p className="muted" role="status">
          확인하는 중
        </p>
      ) : null}
      {phase.kind === "signedOut" ? <LoginForm client={client} notice={phase.notice} onSignedIn={check} /> : null}
      {phase.kind === "signedIn" ? (
        <>
          <header className="topbar">
            <h1>행정법 질의</h1>
            <div className="topbar-tools">
              <span className="who-am-i">{phase.loginId}</span>
              <PrefsControl client={client} />
              <button type="button" onClick={signOut} disabled={streaming}>
                로그아웃
              </button>
            </div>
          </header>
          <Workspace client={client} onStreaming={setStreaming} />
        </>
      ) : null}
    </div>
  );
}
