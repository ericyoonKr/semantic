from dataclasses import dataclass

SPANS: tuple[tuple[str, str], ...] = (("<|channel>", "<channel|>"), ("<think>", "</think>"))
STRAY: tuple[str, ...] = ("<|channel>", "<channel|>", "<think>", "</think>")


def _drop_closers(text: str) -> str:
    for _, close in SPANS:
        text = text.replace(close, "")
    return text


@dataclass
class ThoughtFilter:
    hiding: int | None = None
    held: str = ""

    def push(self, text: str) -> str:
        text = self.held + text
        self.held = ""
        out: list[str] = []
        while text:
            if self.hiding is not None:
                close = SPANS[self.hiding][1]
                at = text.find(close)
                if at < 0:
                    self.held = text[-(len(close) - 1) :] if len(close) > 1 else ""
                    return "".join(out)
                text = text[at + len(close) :]
                self.hiding = None
                continue
            found = [(text.find(o), i) for i, (o, _) in enumerate(SPANS) if text.find(o) >= 0]
            if not found:
                longest = min(len(text), max(len(o) for o, _ in SPANS) - 1)
                keep = max((k for k in range(1, longest + 1) if any(o.startswith(text[-k:]) for o, _ in SPANS)), default=0)
                out.append(_drop_closers(text[: len(text) - keep] if keep else text))
                self.held = text[len(text) - keep :] if keep else ""
                return "".join(out)
            at, i = min(found)
            out.append(_drop_closers(text[:at]))
            text = text[at + len(SPANS[i][0]) :]
            self.hiding = i
        return "".join(out)

    def flush(self) -> str:
        rest, self.held = ("" if self.hiding is not None else self.held), ""
        for mark in STRAY:
            rest = rest.replace(mark, "")
        return rest
