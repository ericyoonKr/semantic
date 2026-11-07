import { ApiError, FALLBACK_TEXT, OFFLINE_TEXT, SIGN_IN_TEXT, errorFrom } from "./errors";
import type { TokenStore } from "./tokenStore";

export type Method = "GET" | "POST" | "DELETE";

export type SendOptions = {
  json?: unknown;
  form?: FormData;
  signal?: AbortSignal;
  anonymous?: boolean;
  accept?: string;
};

export type ClientOptions = {
  tokens: TokenStore;
  fetch?: typeof fetch;
  onUnauthorized?: () => void;
};

export class ApiClient {
  private readonly tokens: TokenStore;
  private readonly fetcher: typeof fetch;
  private unauthorized: (() => void) | undefined;

  constructor(options: ClientOptions) {
    this.tokens = options.tokens;
    this.fetcher = options.fetch ?? ((input, init) => fetch(input, init));
    this.unauthorized = options.onUnauthorized;
  }

  onUnauthorized(handler: (() => void) | undefined): void {
    this.unauthorized = handler;
  }

  hasCsrf(): boolean {
    return this.tokens.get() !== null;
  }

  setCsrf(token: string | null): void {
    this.tokens.set(token);
  }

  async send(method: Method, path: string, options: SendOptions = {}): Promise<Response> {
    const headers = new Headers({ accept: options.accept ?? "application/json" });
    const init: RequestInit = { method, headers, credentials: "same-origin", cache: "no-store" };
    if (options.json !== undefined) {
      headers.set("content-type", "application/json");
      init.body = JSON.stringify(options.json);
    } else if (options.form) {
      init.body = options.form;
    }
    if (options.signal) {
      init.signal = options.signal;
    }
    if (method !== "GET" && !options.anonymous) {
      const token = this.tokens.get();
      if (token === null) {
        this.signOut();
        throw new ApiError(401, SIGN_IN_TEXT);
      }
      headers.set("x-csrf-token", token);
    }
    let res: Response;
    try {
      res = await this.fetcher(path, init);
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") {
        throw error;
      }
      throw new ApiError(0, OFFLINE_TEXT);
    }
    if (!res.ok) {
      const error = await errorFrom(res);
      if (res.status === 401 && !options.anonymous) {
        this.signOut();
      }
      throw error;
    }
    return res;
  }

  async json<T>(method: Method, path: string, options: SendOptions = {}): Promise<T> {
    const res = await this.send(method, path, options);
    try {
      return (await res.json()) as T;
    } catch {
      throw new ApiError(res.status, FALLBACK_TEXT);
    }
  }

  async empty(method: Method, path: string, options: SendOptions = {}): Promise<void> {
    await this.send(method, path, options);
  }

  private signOut(): void {
    this.tokens.set(null);
    this.unauthorized?.();
  }
}
