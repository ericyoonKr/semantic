import type { ManagedUser, UserAction, UserStatus } from "./api";

export function statusLabel(status: UserStatus): string {
  switch (status) {
    case "pending":
      return "대기";
    case "active":
      return "활성";
    case "disabled":
      return "꺼짐";
  }
}

export function whenText(iso: string): string {
  const at = Date.parse(iso);
  if (Number.isNaN(at)) {
    return iso;
  }
  return new Date(at).toLocaleString("ko-KR", { dateStyle: "medium", timeStyle: "short" });
}

export function splitUsers(users: ManagedUser[]): { pending: ManagedUser[]; others: ManagedUser[] } {
  return { pending: users.filter((u) => u.status === "pending"), others: users.filter((u) => u.status !== "pending") };
}

export function confirmText(action: UserAction, user: ManagedUser): string | null {
  switch (action) {
    case "approve":
      return null;
    case "reject":
      return user.loginId + "의 신청을 거절할까요? 신청 기록이 지워집니다.";
    case "disable":
      return user.loginId + "을(를) 끌까요? 다시 로그인할 수 없게 됩니다.";
  }
}
