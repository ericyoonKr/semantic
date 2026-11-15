import { useCallback, useEffect, useRef, useState } from "react";
import type { ApiClient } from "../../shared/api/client";
import { messageOf } from "../../shared/api/errors";
import { ErrorNote } from "../../shared/ui/ErrorNote";
import { Icon } from "../../shared/ui/Icon";
import { actOnUser, listUsers, type ManagedUser, type UserAction } from "./api";
import { confirmText, splitUsers, statusLabel, whenText } from "./users";
import "./auth.css";

type Props = {
  client: ApiClient;
  selfLoginId: string;
  onClose: () => void;
};

export function ApprovalPanel({ client, selfLoginId, onClose }: Props) {
  const closer = useRef<HTMLButtonElement>(null);
  const [users, setUsers] = useState<ManagedUser[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setUsers(await listUsers(client));
  }, [client]);

  useEffect(() => {
    closer.current?.focus();
    refresh().catch((err: unknown) => setError(messageOf(err)));
  }, [refresh]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  async function act(action: UserAction, user: ManagedUser) {
    if (busy) {
      return;
    }
    const ask = confirmText(action, user);
    if (ask !== null && !window.confirm(ask)) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await actOnUser(client, action, user.loginId);
      await refresh();
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  const { pending, others } = splitUsers(users ?? []);

  return (
    <div className="approval-backdrop" onClick={onClose}>
      <section className="approval" role="dialog" aria-modal="true" aria-labelledby="approval-title" onClick={(e) => e.stopPropagation()}>
        <header className="approval-head">
          <h2 id="approval-title">가입 승인</h2>
          <button ref={closer} type="button" className="icon-button" onClick={onClose} aria-label="가입 승인 닫기">
            <Icon name="close" />
          </button>
        </header>
        {users === null && !error ? (
          <p className="muted" role="status">
            불러오는 중
          </p>
        ) : null}
        <ErrorNote message={error} />
        {users !== null ? (
          <>
            <h3 className="approval-sub">
              <span>대기 중</span>
              <span className="approval-count">{pending.length}</span>
            </h3>
            {pending.length === 0 ? (
              <p className="muted">대기 중인 신청이 없습니다.</p>
            ) : (
              <div className="approval-scroll">
                <table className="approval-table" aria-label="대기 중인 신청">
                  <thead>
                    <tr>
                      <th scope="col">아이디</th>
                      <th scope="col">이름</th>
                      <th scope="col">신청 시각</th>
                      <th scope="col">처리</th>
                    </tr>
                  </thead>
                  <tbody>
                    {pending.map((u) => (
                      <tr key={u.loginId}>
                        <td>{u.loginId}</td>
                        <td>{u.name}</td>
                        <td>{whenText(u.requestedAt)}</td>
                        <td className="approval-actions">
                          <button type="button" className="button-primary" disabled={busy} onClick={() => act("approve", u)} aria-label={u.loginId + " 승인"}>
                            승인
                          </button>
                          <button type="button" className="button-secondary" disabled={busy} onClick={() => act("reject", u)} aria-label={u.loginId + " 거절"}>
                            거절
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            <h3 className="approval-sub">
              <span>사용자</span>
              <span className="approval-count">{others.length}</span>
            </h3>
            {others.length === 0 ? (
              <p className="muted">사용자가 없습니다.</p>
            ) : (
              <div className="approval-scroll">
                <table className="approval-table" aria-label="사용자 목록">
                  <thead>
                    <tr>
                      <th scope="col">아이디</th>
                      <th scope="col">이름</th>
                      <th scope="col">상태</th>
                      <th scope="col">신청 시각</th>
                      <th scope="col">처리</th>
                    </tr>
                  </thead>
                  <tbody>
                    {others.map((u) => (
                      <tr key={u.loginId}>
                        <td>{u.loginId}</td>
                        <td>{u.name}</td>
                        <td>
                          <span className="approval-status" data-status={u.status}>
                            {statusLabel(u.status)}
                          </span>
                        </td>
                        <td>{whenText(u.requestedAt)}</td>
                        <td className="approval-actions">
                          {u.status === "active" && u.loginId !== selfLoginId ? (
                            <button type="button" className="button-secondary" disabled={busy} onClick={() => act("disable", u)} aria-label={u.loginId + " 끄기"}>
                              끄기
                            </button>
                          ) : null}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </>
        ) : null}
      </section>
    </div>
  );
}
