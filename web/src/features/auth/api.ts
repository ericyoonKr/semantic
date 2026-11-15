import type { ApiClient } from "../../shared/api/client";
import { ApiError, FALLBACK_TEXT } from "../../shared/api/errors";

export type Me = { loginId: string; name: string; isMaster: boolean };

export type UserStatus = "pending" | "active" | "disabled";

export type ManagedUser = { loginId: string; name: string; status: UserStatus; requestedAt: string };

export type UserAction = "approve" | "reject" | "disable";

export async function login(client: ApiClient, loginId: string, password: string): Promise<void> {
  const body = await client.json<{ csrf_token?: unknown }>("POST", "/api/auth/login", {
    json: { login_id: loginId, password },
    anonymous: true,
  });
  if (typeof body.csrf_token !== "string" || body.csrf_token === "") {
    throw new ApiError(500, FALLBACK_TEXT);
  }
  client.setCsrf(body.csrf_token);
}

export async function signup(client: ApiClient, loginId: string, password: string, name: string): Promise<void> {
  await client.empty("POST", "/api/auth/signup", {
    json: { login_id: loginId, password, name },
    anonymous: true,
  });
}

export async function logout(client: ApiClient): Promise<void> {
  try {
    await client.empty("POST", "/api/auth/logout");
  } finally {
    client.setCsrf(null);
  }
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

export async function me(client: ApiClient): Promise<Me> {
  const body = await client.json<{ login_id?: unknown; name?: unknown; is_master?: unknown }>("GET", "/api/auth/me");
  return { loginId: text(body.login_id), name: text(body.name), isMaster: body.is_master === true };
}

function statusOf(value: unknown): UserStatus {
  return value === "pending" || value === "disabled" ? value : "active";
}

export async function listUsers(client: ApiClient): Promise<ManagedUser[]> {
  const body = await client.json<unknown>("GET", "/api/admin/users");
  if (!Array.isArray(body)) {
    throw new ApiError(500, FALLBACK_TEXT);
  }
  return body.map((row: unknown) => {
    const r = (row !== null && typeof row === "object" ? row : {}) as Record<string, unknown>;
    return { loginId: text(r.login_id), name: text(r.name), status: statusOf(r.status), requestedAt: text(r.requested_at) };
  });
}

export async function actOnUser(client: ApiClient, action: UserAction, loginId: string): Promise<void> {
  await client.empty("POST", "/api/admin/users/" + encodeURIComponent(loginId) + "/" + action);
}
