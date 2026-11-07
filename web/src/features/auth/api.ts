import type { ApiClient } from "../../shared/api/client";
import { ApiError, FALLBACK_TEXT } from "../../shared/api/errors";

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

export async function logout(client: ApiClient): Promise<void> {
  try {
    await client.empty("POST", "/api/auth/logout");
  } finally {
    client.setCsrf(null);
  }
}

export async function me(client: ApiClient): Promise<string> {
  const body = await client.json<{ login_id?: unknown }>("GET", "/api/auth/me");
  return typeof body.login_id === "string" ? body.login_id : "";
}
