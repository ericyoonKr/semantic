export type SseMessage = {
  event: string;
  data: string;
};

const CR = "\r";
const LF = "\n";

export class SseParser {
  private buffer = "";
  private event = "";
  private data: string[] = [];

  push(chunk: string): SseMessage[] {
    this.buffer += chunk;
    const out: SseMessage[] = [];
    let start = 0;
    for (let i = 0; i < this.buffer.length; i++) {
      const ch = this.buffer[i];
      if (ch !== LF && ch !== CR) {
        continue;
      }
      if (ch === CR && i === this.buffer.length - 1) {
        break;
      }
      const message = this.line(this.buffer.slice(start, i));
      if (message) {
        out.push(message);
      }
      if (ch === CR && this.buffer[i + 1] === LF) {
        i++;
      }
      start = i + 1;
    }
    this.buffer = this.buffer.slice(start);
    return out;
  }

  end(): SseMessage[] {
    const held = this.buffer;
    this.buffer = "";
    const message = held.endsWith(CR) ? this.line(held.slice(0, -1)) : null;
    this.event = "";
    this.data = [];
    return message ? [message] : [];
  }

  private line(line: string): SseMessage | null {
    if (line === "") {
      return this.dispatch();
    }
    if (line.startsWith(":")) {
      return null;
    }
    const colon = line.indexOf(":");
    const field = colon < 0 ? line : line.slice(0, colon);
    let value = colon < 0 ? "" : line.slice(colon + 1);
    if (value.startsWith(" ")) {
      value = value.slice(1);
    }
    if (field === "event") {
      this.event = value;
    } else if (field === "data") {
      this.data.push(value);
    }
    return null;
  }

  private dispatch(): SseMessage | null {
    const event = this.event || "message";
    const data = this.data;
    this.event = "";
    this.data = [];
    if (data.length === 0) {
      return null;
    }
    return { event, data: data.join(LF) };
  }
}

export async function readSse(body: ReadableStream<Uint8Array>, onMessage: (message: SseMessage) => boolean | void): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder("utf-8");
  const parser = new SseParser();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      const text = done ? decoder.decode() : decoder.decode(value, { stream: true });
      const messages = parser.push(text);
      if (done) {
        messages.push(...parser.end());
      }
      for (const message of messages) {
        if (onMessage(message) === false) {
          await reader.cancel().catch(() => undefined);
          return;
        }
      }
      if (done) {
        return;
      }
    }
  } finally {
    reader.releaseLock();
  }
}
