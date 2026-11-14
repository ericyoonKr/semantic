import { describe, expect, it } from "vitest";
import type { UploadSummary } from "../src/features/upload/api";
import { BLOCKED_TEXT, REJECTED_TEXT, UploadQueue, guardWindow, hasFiles, isAccepted, sessionQueue, type QueueItem } from "../src/features/upload/dropQueue";
import { ApiError } from "../src/shared/api/errors";

function file(name: string): File {
  return new File(["x"], name);
}

function summary(name: string, pages: number): UploadSummary {
  return { fileId: "f-" + name, filename: name, format: "pdf", pages, outline: [], warnings: [], findingsCount: 0 };
}

type Gate = { name: string; release: () => void; fail: (err: unknown) => void };

function harness() {
  const started: Gate[] = [];
  const snapshots: QueueItem[][] = [];
  const busy: boolean[] = [];
  const uploaded: string[] = [];
  let inFlight = 0;
  let maxInFlight = 0;
  const upload = (f: File, signal: AbortSignal) =>
    new Promise<UploadSummary>((resolve, reject) => {
      inFlight += 1;
      maxInFlight = Math.max(maxInFlight, inFlight);
      signal.addEventListener("abort", () => {
        inFlight -= 1;
        reject(new DOMException("aborted", "AbortError"));
      });
      started.push({
        name: f.name,
        release: () => {
          inFlight -= 1;
          resolve(summary(f.name, f.name.length));
        },
        fail: (err) => {
          inFlight -= 1;
          reject(err);
        },
      });
    });
  const queue = new UploadQueue(upload, {
    change: (items) => snapshots.push(items),
    busy: (b) => busy.push(b),
    uploaded: (s) => uploaded.push(s.filename),
  });
  return { queue, started, snapshots, busy, uploaded, maxInFlight: () => maxInFlight };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("isAccepted", () => {
  it("accepts pdf, docx and hwpx in any letter case", () => {
    expect(["a.pdf", "b.docx", "c.hwpx", "D.PDF", "보고서.DocX", "x.y.hwpx"].map(isAccepted)).toEqual([true, true, true, true, true, true]);
  });

  it("rejects other types and names that only look similar", () => {
    expect(["a.doc", "b.hwp", "c.txt", "d.pdf.exe", "pdf", ".pdf", "e.docxx", "f", "g.zip"].map(isAccepted)).toEqual([false, false, false, false, false, false, false, false, false]);
  });
});

describe("hasFiles", () => {
  it("is true only when the drag carries files", () => {
    expect(hasFiles(["Files"])).toBe(true);
    expect(hasFiles(["text/plain", "Files"])).toBe(true);
    expect(hasFiles(["text/plain", "text/html"])).toBe(false);
    expect(hasFiles([])).toBe(false);
    expect(hasFiles(null)).toBe(false);
  });
});

describe("UploadQueue", () => {
  it("uploads accepted files one after another in drop order", async () => {
    const h = harness();
    expect(h.queue.offer([file("1.pdf"), file("2.docx"), file("3.hwpx")], false)).toBe("queued");
    await tick();
    expect(h.started.map((g) => g.name)).toEqual(["1.pdf"]);
    h.started[0]?.release();
    await tick();
    expect(h.started.map((g) => g.name)).toEqual(["1.pdf", "2.docx"]);
    h.started[1]?.release();
    await tick();
    h.started[2]?.release();
    await h.queue.idle();
    expect(h.maxInFlight()).toBe(1);
    expect(h.uploaded).toEqual(["1.pdf", "2.docx", "3.hwpx"]);
    expect(h.queue.snapshot().map((i) => [i.name, i.state, i.pages])).toEqual([
      ["1.pdf", "done", 5],
      ["2.docx", "done", 6],
      ["3.hwpx", "done", 6],
    ]);
    expect(h.busy).toEqual([true, false]);
  });

  it("shows waiting, uploading and done states as they happen", async () => {
    const h = harness();
    h.queue.offer([file("a.pdf"), file("b.pdf")], false);
    await tick();
    expect(h.queue.snapshot().map((i) => i.state)).toEqual(["uploading", "waiting"]);
    h.started[0]?.release();
    await tick();
    expect(h.queue.snapshot().map((i) => i.state)).toEqual(["done", "uploading"]);
    h.started[1]?.release();
    await h.queue.idle();
    expect(h.queue.snapshot().map((i) => i.state)).toEqual(["done", "done"]);
  });

  it("marks other types as rejected with a message and never uploads them", async () => {
    const h = harness();
    h.queue.offer([file("memo.txt"), file("ok.pdf"), file("old.hwp")], false);
    await tick();
    h.started[0]?.release();
    await h.queue.idle();
    expect(h.started.map((g) => g.name)).toEqual(["ok.pdf"]);
    expect(h.queue.snapshot().map((i) => [i.name, i.state, i.message])).toEqual([
      ["memo.txt", "rejected", REJECTED_TEXT],
      ["ok.pdf", "done", null],
      ["old.hwp", "rejected", REJECTED_TEXT],
    ]);
  });

  it("does not start any upload when only rejected files are dropped", async () => {
    const h = harness();
    expect(h.queue.offer([file("a.png")], false)).toBe("queued");
    await h.queue.idle();
    expect(h.started).toEqual([]);
    expect(h.busy).toEqual([]);
  });

  it("refuses the whole drop while an answer is streaming", async () => {
    const h = harness();
    expect(h.queue.offer([file("a.pdf"), file("b.docx")], true)).toBe("blocked");
    await tick();
    expect(h.started).toEqual([]);
    expect(h.queue.snapshot()).toEqual([]);
    expect(h.snapshots).toEqual([]);
    expect(BLOCKED_TEXT).toBe("답을 받는 동안에는 파일을 올릴 수 없습니다");
  });

  it("keeps going after a failed upload and shows the server message", async () => {
    const h = harness();
    h.queue.offer([file("big.pdf"), file("next.pdf")], false);
    await tick();
    h.started[0]?.fail(new ApiError(413, "파일이 너무 큽니다"));
    await tick();
    h.started[1]?.release();
    await h.queue.idle();
    expect(h.queue.snapshot().map((i) => [i.name, i.state, i.message])).toEqual([
      ["big.pdf", "failed", "파일이 너무 큽니다"],
      ["next.pdf", "done", null],
    ]);
    expect(h.uploaded).toEqual(["next.pdf"]);
  });

  it("appends files dropped during an upload to the same line instead of running them in parallel", async () => {
    const h = harness();
    h.queue.offer([file("first.pdf")], false);
    await tick();
    h.queue.offer([file("second.pdf")], false);
    await tick();
    expect(h.started.map((g) => g.name)).toEqual(["first.pdf"]);
    h.started[0]?.release();
    await tick();
    h.started[1]?.release();
    await h.queue.idle();
    expect(h.maxInFlight()).toBe(1);
    expect(h.uploaded).toEqual(["first.pdf", "second.pdf"]);
    expect(h.busy).toEqual([true, false]);
  });

  it("lets finished entries be dismissed but not running ones", async () => {
    const h = harness();
    h.queue.offer([file("a.pdf"), file("bad.txt")], false);
    await tick();
    const [running, rejected] = h.queue.snapshot();
    h.queue.dismiss(running?.id ?? -1);
    h.queue.dismiss(rejected?.id ?? -1);
    expect(h.queue.snapshot().map((i) => i.name)).toEqual(["a.pdf"]);
    h.started[0]?.release();
    await h.queue.idle();
  });

  it("keeps uploading every file while nobody watches and shows the progress to the next viewer", async () => {
    const h = harness();
    h.queue.offer([file("a.pdf"), file("b.pdf")], false);
    await tick();
    h.queue.listen(null);
    const count = h.snapshots.length;
    h.started[0]?.release();
    await tick();
    h.started[1]?.release();
    await h.queue.idle();
    expect(h.started.map((g) => g.name)).toEqual(["a.pdf", "b.pdf"]);
    expect(h.snapshots.length).toBe(count);
    const seen: QueueItem[][] = [];
    const busy: boolean[] = [];
    h.queue.listen({ change: (items) => seen.push(items), busy: (b) => busy.push(b), uploaded: () => undefined });
    expect(seen[0]?.map((i) => [i.name, i.state])).toEqual([
      ["a.pdf", "done"],
      ["b.pdf", "done"],
    ]);
    expect(busy).toEqual([false]);
  });

  it("gives one queue per conversation that lives across views", () => {
    const owner = {};
    const make = () => new UploadQueue(async () => summary("x.pdf", 1));
    const first = sessionQueue(owner, "s1", make);
    expect(sessionQueue(owner, "s1", make)).toBe(first);
    expect(sessionQueue(owner, "s2", make)).not.toBe(first);
    expect(sessionQueue({}, "s1", make)).not.toBe(first);
  });
});

describe("UploadQueue cancel", () => {
  it("stops the file being sent and drops a waiting one, then goes on with the rest", async () => {
    const h = harness();
    h.queue.offer([file("a.pdf"), file("b.pdf"), file("c.pdf")], false);
    await tick();
    const [a, b, c] = h.queue.snapshot();
    h.queue.cancel(b?.id ?? 0);
    h.queue.cancel(a?.id ?? 0);
    await tick();
    expect(h.queue.snapshot().map((i) => [i.name, i.state])).toEqual([
      ["a.pdf", "canceled"],
      ["b.pdf", "canceled"],
      ["c.pdf", "uploading"],
    ]);
    h.started.find((g) => g.name === "c.pdf")?.release();
    await h.queue.idle();
    expect(h.uploaded).toEqual(["c.pdf"]);
    expect(h.started.map((g) => g.name)).toEqual(["a.pdf", "c.pdf"]);
    h.queue.dismiss(c?.id ?? 0);
    h.queue.dismiss(a?.id ?? 0);
    expect(h.queue.snapshot().map((i) => i.name)).toEqual(["b.pdf"]);
  });
});

describe("guardWindow", () => {
  function dragEvent(type: string) {
    const e = new Event(type, { cancelable: true, bubbles: true });
    const transfer = { dropEffect: "copy" };
    Object.defineProperty(e, "dataTransfer", { value: transfer });
    return { e, transfer };
  }

  it("stops the browser from opening a file dropped outside the drop zone", () => {
    const target = new EventTarget();
    const stop = guardWindow(target);
    const over = dragEvent("dragover");
    const drop = dragEvent("drop");
    target.dispatchEvent(over.e);
    target.dispatchEvent(drop.e);
    expect(over.e.defaultPrevented).toBe(true);
    expect(over.transfer.dropEffect).toBe("none");
    expect(drop.e.defaultPrevented).toBe(true);
    stop();
    const later = dragEvent("drop");
    target.dispatchEvent(later.e);
    expect(later.e.defaultPrevented).toBe(false);
  });

  it("leaves the drop effect alone when the drop zone already handled the event", () => {
    const target = new EventTarget();
    const stop = guardWindow(target);
    const over = dragEvent("dragover");
    over.e.preventDefault();
    target.dispatchEvent(over.e);
    expect(over.transfer.dropEffect).toBe("copy");
    stop();
  });
});

describe("dismissFinished", () => {
  it("clears finished chips but keeps the ones still waiting or uploading", async () => {
    const h = harness();
    h.queue.offer([file("a.pdf"), file("b.docx"), file("c.hwpx")], false);
    await tick();
    h.started[0]?.release();
    await tick();
    const before = h.snapshots[h.snapshots.length - 1] ?? [];
    expect(before.map((i) => i.state)).toEqual(["done", "uploading", "waiting"]);
    h.queue.dismissFinished();
    const after = h.snapshots[h.snapshots.length - 1] ?? [];
    expect(after.map((i) => i.name)).toEqual(["b.docx", "c.hwpx"]);
    const count = h.snapshots.length;
    h.queue.dismissFinished();
    expect(h.snapshots.length).toBe(count);
  });
});
