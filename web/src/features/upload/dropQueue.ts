import { messageOf } from "../../shared/api/errors";
import type { UploadSummary } from "./api";

export const ACCEPTED_EXTENSIONS = [".pdf", ".docx", ".hwpx"] as const;
export const ACCEPT_ATTR = ACCEPTED_EXTENSIONS.join(",");
export const REJECTED_TEXT = "PDF, DOCX, HWPX 파일만 올릴 수 있습니다";
export const BLOCKED_TEXT = "답을 받는 동안에는 파일을 올릴 수 없습니다";

export function isAccepted(name: string): boolean {
  const lower = name.toLowerCase();
  return ACCEPTED_EXTENSIONS.some((ext) => lower.endsWith(ext) && lower.length > ext.length);
}

export function hasFiles(types: readonly string[] | DOMStringList | null | undefined): boolean {
  if (!types) {
    return false;
  }
  return Array.from(types as ArrayLike<string>).includes("Files");
}

export type QueueState = "waiting" | "uploading" | "done" | "failed" | "rejected";

export type QueueItem = {
  id: number;
  name: string;
  state: QueueState;
  message: string | null;
  pages: number | null;
};

export type OfferResult = "blocked" | "queued" | "empty";

export type QueueListener = {
  change: (items: QueueItem[]) => void;
  busy: (busy: boolean) => void;
  uploaded: (summary: UploadSummary) => void;
};

type Pending = { id: number; file: File };

export class UploadQueue {
  private readonly upload: (file: File) => Promise<UploadSummary>;
  private readonly listener: QueueListener;
  private items: QueueItem[] = [];
  private pending: Pending[] = [];
  private nextId = 1;
  private running: Promise<void> | null = null;
  private closed = false;

  constructor(upload: (file: File) => Promise<UploadSummary>, listener: QueueListener) {
    this.upload = upload;
    this.listener = listener;
  }

  offer(files: readonly File[], blocked: boolean): OfferResult {
    if (this.closed || files.length === 0) {
      return "empty";
    }
    if (blocked) {
      return "blocked";
    }
    for (const file of files) {
      const id = this.nextId++;
      if (isAccepted(file.name)) {
        this.items = [...this.items, { id, name: file.name, state: "waiting", message: null, pages: null }];
        this.pending.push({ id, file });
      } else {
        this.items = [...this.items, { id, name: file.name, state: "rejected", message: REJECTED_TEXT, pages: null }];
      }
    }
    this.emit();
    if (this.running === null && this.pending.length > 0) {
      this.running = this.drain();
    }
    return "queued";
  }

  dismiss(id: number): void {
    const item = this.items.find((i) => i.id === id);
    if (!item || item.state === "waiting" || item.state === "uploading") {
      return;
    }
    this.items = this.items.filter((i) => i.id !== id);
    this.emit();
  }

  snapshot(): QueueItem[] {
    return this.items;
  }

  idle(): Promise<void> {
    return this.running ?? Promise.resolve();
  }

  close(): void {
    this.closed = true;
    this.pending = [];
  }

  private set(id: number, patch: Partial<QueueItem>): void {
    this.items = this.items.map((i) => (i.id === id ? { ...i, ...patch } : i));
    this.emit();
  }

  private emit(): void {
    if (!this.closed) {
      this.listener.change(this.items);
    }
  }

  private async drain(): Promise<void> {
    if (!this.closed) {
      this.listener.busy(true);
    }
    try {
      let next = this.pending.shift();
      while (next && !this.closed) {
        this.set(next.id, { state: "uploading" });
        try {
          const summary = await this.upload(next.file);
          this.set(next.id, { state: "done", pages: summary.pages });
          if (!this.closed) {
            this.listener.uploaded(summary);
          }
        } catch (err) {
          this.set(next.id, { state: "failed", message: messageOf(err) });
        }
        next = this.pending.shift();
      }
    } finally {
      this.running = null;
      if (!this.closed) {
        this.listener.busy(false);
      }
    }
  }
}

export function guardWindow(target: EventTarget): () => void {
  const over = (e: Event) => {
    if (e.defaultPrevented) {
      return;
    }
    e.preventDefault();
    const transfer = (e as DragEvent).dataTransfer;
    if (transfer) {
      transfer.dropEffect = "none";
    }
  };
  const drop = (e: Event) => {
    if (!e.defaultPrevented) {
      e.preventDefault();
    }
  };
  target.addEventListener("dragover", over);
  target.addEventListener("drop", drop);
  return () => {
    target.removeEventListener("dragover", over);
    target.removeEventListener("drop", drop);
  };
}
