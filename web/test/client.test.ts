import { describe, expect, it } from "vitest";
import { login, logout } from "../src/features/auth/api";
import { ApiClient } from "../src/shared/api/client";
import { ApiError, FALLBACK_TEXT, OFFLINE_TEXT, SIGN_IN_TEXT } from "../src/shared/api/errors";
import { memoryStore } from "../src/shared/api/tokenStore";
import { fakeFetch, json, type Call } from "./fakeFetch";

function setup(token: string | null, respond: (call: Call) => Response | Promise<Response>) {
  const tokens = memoryStore(token);
  const fetch = fakeFetch(respond);
  let signedOut = 0;
  const client = new ApiClient({ tokens, fetch: fetch.fn, onUnauthorized: () => signedOut++ });
  return { client, tokens, calls: fetch.calls, signedOut: () => signedOut };
}

async function failure(p: Promise<unknown>): Promise<ApiError> {
  try {
    await p;
  } catch (e) {
    if (e instanceof ApiError) {
      return e;
    }
    throw e;
  }
  throw new Error("expected failure");
}

describe("ApiClient CSRF header", () => {
  it("sends no CSRF header on GET and uses same-origin cookies", async () => {
    const t = setup("tok", () => json(200, []));
    await t.client.json("GET", "/api/sessions");
    expect(t.calls[0]?.headers.has("x-csrf-token")).toBe(false);
    expect(t.calls[0]?.credentials).toBe("same-origin");
  });

  it("sends the CSRF token on POST and DELETE", async () => {
    const t = setup("tok", (c) => (c.method === "DELETE" ? new Response(null, { status: 204 }) : json(200, { session_id: "s" })));
    await t.client.json("POST", "/session");
    await t.client.empty("DELETE", "/prefs");
    expect(t.calls.map((c) => [c.method, c.headers.get("x-csrf-token")])).toEqual([
      ["POST", "tok"],
      ["DELETE", "tok"],
    ]);
  });

  it("does not send a change request without a token and asks to sign in", async () => {
    const t = setup(null, () => json(200, {}));
    const e = await failure(t.client.json("POST", "/session"));
    expect(e.status).toBe(401);
    expect(e.message).toBe(SIGN_IN_TEXT);
    expect(t.calls).toHaveLength(0);
    expect(t.signedOut()).toBe(1);
  });

  it("logs in without a CSRF header and stores the returned token", async () => {
    const t = setup(null, () => json(200, { csrf_token: "new" }));
    await login(t.client, "grader", "pw");
    expect(t.calls[0]?.headers.has("x-csrf-token")).toBe(false);
    expect(JSON.parse(String(t.calls[0]?.body))).toEqual({ login_id: "grader", password: "pw" });
    expect(t.tokens.get()).toBe("new");
  });

  it("clears the token on logout even when the request fails", async () => {
    const t = setup("tok", () => json(502, { message: "x" }));
    await failure(logout(t.client));
    expect(t.calls[0]?.headers.get("x-csrf-token")).toBe("tok");
    expect(t.tokens.get()).toBeNull();
  });
});

describe("ApiClient errors", () => {
  it("uses the gateway message and status", async () => {
    const t = setup("tok", () => json(409, { message: "이 대화에서 답을 쓰는 중입니다" }));
    const e = await failure(t.client.json("POST", "/session"));
    expect([e.status, e.message]).toEqual([409, "이 대화에서 답을 쓰는 중입니다"]);
    expect(t.signedOut()).toBe(0);
  });

  it("falls back to a general message when the body is not the gateway shape", async () => {
    for (const res of [new Response("<html>", { status: 500 }), json(400, { error: "x" }), json(400, { message: "" })]) {
      const t = setup("tok", () => res);
      const e = await failure(t.client.json("GET", "/api/sessions"));
      expect(e.message).toBe(FALLBACK_TEXT);
    }
  });

  it("signs out on 401 for signed requests and clears the token", async () => {
    const t = setup("tok", () => json(401, { message: "로그인이 필요합니다" }));
    await failure(t.client.json("GET", "/api/auth/me"));
    expect(t.signedOut()).toBe(1);
    expect(t.tokens.get()).toBeNull();
  });

  it("does not sign out on a failed login", async () => {
    const t = setup(null, () => json(401, { message: "아이디 또는 비밀번호가 맞지 않습니다" }));
    const e = await failure(login(t.client, "a", "b"));
    expect(e.message).toBe("아이디 또는 비밀번호가 맞지 않습니다");
    expect(t.signedOut()).toBe(0);
  });

  it("reports a network failure with the offline message", async () => {
    const t = setup("tok", () => Promise.reject(new TypeError("failed")));
    const e = await failure(t.client.json("GET", "/api/sessions"));
    expect([e.status, e.message]).toEqual([0, OFFLINE_TEXT]);
  });

  it("rejects a login answer without a token", async () => {
    const t = setup(null, () => json(200, {}));
    const e = await failure(login(t.client, "a", "b"));
    expect(e.message).toBe(FALLBACK_TEXT);
    expect(t.tokens.get()).toBeNull();
  });
});
