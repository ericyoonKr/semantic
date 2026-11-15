import { useCallback, useEffect, useMemo, useState } from "react";
import { ApprovalPanel } from "../features/auth/ApprovalPanel";
import { logout, me, type Me } from "../features/auth/api";
import { LoginForm } from "../features/auth/LoginForm";
import { PrefsControl } from "../features/prefs/PrefsControl";
import { guardWindow } from "../features/upload/dropQueue";
import { ApiClient } from "../shared/api/client";
import { SIGN_IN_TEXT } from "../shared/api/errors";
import { sessionStore } from "../shared/api/tokenStore";
import { Disclaimer } from "../shared/ui/Disclaimer";
import { Workspace } from "./Workspace";

type Phase = { kind: "checking" } | { kind: "signedOut"; notice: string | null } | { kind: "signedIn"; me: Me };

const CSRF_KEY = "csrf";

export function App() {
  const client = useMemo(() => new ApiClient({ tokens: sessionStore(CSRF_KEY) }), []);
  const [phase, setPhase] = useState<Phase>({ kind: "checking" });
  const [streaming, setStreaming] = useState(false);
  const [approvalsOpen, setApprovalsOpen] = useState(false);

  const check = useCallback(() => {
    if (!client.hasCsrf()) {
      setPhase({ kind: "signedOut", notice: null });
      return;
    }
    me(client)
      .then((who) => setPhase({ kind: "signedIn", me: who }))
      .catch(() => setPhase({ kind: "signedOut", notice: null }));
  }, [client]);

  const closeApprovals = useCallback(() => setApprovalsOpen(false), []);

  useEffect(() => guardWindow(window), []);

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
    setApprovalsOpen(false);
    setPhase({ kind: "signedOut", notice: "로그아웃했습니다" });
  }

  return (
    <div className="app">
      <Disclaimer />
      {phase.kind === "checking" ? (
        <p className="muted checking" role="status">
          확인하는 중
        </p>
      ) : null}
      {phase.kind === "signedOut" ? <LoginForm client={client} notice={phase.notice} onSignedIn={check} /> : null}
      {phase.kind === "signedIn" ? (
        <Workspace
          client={client}
          onStreaming={setStreaming}
          account={
            <>
              <div className="account-who">
                <span className="avatar" aria-hidden="true">
                  {phase.me.loginId.slice(0, 1).toUpperCase()}
                </span>
                <span className="who-am-i">{phase.me.loginId}</span>
              </div>
              <div className="account-tools">
                {phase.me.isMaster ? (
                  <button type="button" className="button-secondary" onClick={() => setApprovalsOpen(true)} aria-haspopup="dialog" aria-expanded={approvalsOpen}>
                    가입 승인
                  </button>
                ) : null}
                <PrefsControl client={client} />
                <button type="button" className="button-secondary" onClick={signOut} disabled={streaming}>
                  로그아웃
                </button>
              </div>
            </>
          }
        />
      ) : null}
      {phase.kind === "signedIn" && phase.me.isMaster && approvalsOpen ? <ApprovalPanel client={client} selfLoginId={phase.me.loginId} onClose={closeApprovals} /> : null}
    </div>
  );
}
