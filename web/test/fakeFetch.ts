export type Call = {
  url: string;
  method: string;
  headers: Headers;
  credentials: RequestCredentials | undefined;
  body: BodyInit | null | undefined;
};

export function fakeFetch(respond: (call: Call) => Response | Promise<Response>) {
  const calls: Call[] = [];
  const fn: typeof fetch = async (input, init) => {
    const call: Call = {
      url: String(input),
      method: init?.method ?? "GET",
      headers: new Headers(init?.headers),
      credentials: init?.credentials,
      body: init?.body,
    };
    calls.push(call);
    return respond(call);
  };
  return { fn, calls };
}

export function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

export function stream(chunks: string[], fail = false): ReadableStream<Uint8Array> {
  const enc = new TextEncoder();
  let i = 0;
  return new ReadableStream({
    pull(controller) {
      const chunk = chunks[i++];
      if (chunk !== undefined) {
        controller.enqueue(enc.encode(chunk));
      } else if (fail) {
        controller.error(new TypeError("network"));
      } else {
        controller.close();
      }
    },
  });
}
