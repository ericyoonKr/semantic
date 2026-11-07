import logging
import re
import sys
import threading
from datetime import UTC, date, datetime, timedelta, timezone
from pathlib import Path
from typing import TextIO

KST = timezone(timedelta(hours=9))
MAX_BYTES = 100 * 1024 * 1024
KEEP_DAYS = 14
MASK = "[가림]"
PREFIX = "agent"
FORMAT = "%(asctime)s %(levelname)s %(name)s %(message)s"

_EDGE_BEFORE = r"(?<![0-9A-Za-z\-])"
_EDGE_AFTER = r"(?![0-9A-Za-z\-])"
JOINED = re.compile(_EDGE_BEFORE + r"\d{2,12}(?:-\d{2,12}){1,3}" + _EDGE_AFTER)
PLAIN = re.compile(_EDGE_BEFORE + r"\d{10,14}" + _EDGE_AFTER)
FILE_NAME = re.compile(rf"^{PREFIX}-(\d{{4}}-\d{{2}}-\d{{2}})(?:\.\d+)?\.log$")


def _mask_joined(match: re.Match[str]) -> str:
    digits = sum(ch.isdigit() for ch in match.group(0))
    return MASK if 9 <= digits <= 16 else match.group(0)


def mask(text: str) -> str:
    return PLAIN.sub(MASK, JOINED.sub(_mask_joined, text))


class MaskingFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        return mask(super().format(record))


def today_kst(now: datetime | None = None) -> date:
    return (now or datetime.now(UTC)).astimezone(KST).date()


def log_path(folder: Path, day: date, part: int) -> Path:
    suffix = f".{part}" if part else ""
    return folder / f"{PREFIX}-{day.isoformat()}{suffix}.log"


def prune(folder: Path, today: date, keep_days: int = KEEP_DAYS) -> list[Path]:
    removed = []
    for path in folder.glob(f"{PREFIX}-*.log"):
        match = FILE_NAME.match(path.name)
        if match is None:
            continue
        try:
            day = date.fromisoformat(match.group(1))
        except ValueError:
            continue
        if (today - day).days >= keep_days:
            try:
                path.unlink()
                removed.append(path)
            except OSError:
                continue
    return removed


class DailySizeHandler(logging.Handler):
    def __init__(self, folder: Path, max_bytes: int = MAX_BYTES, keep_days: int = KEEP_DAYS) -> None:
        super().__init__()
        self.folder = folder
        self.max_bytes = max_bytes
        self.keep_days = keep_days
        self.day: date | None = None
        self.part = 0
        self.stream: TextIO | None = None
        self.size = 0
        self.guard = threading.Lock()
        folder.mkdir(parents=True, exist_ok=True)

    def _open(self, day: date) -> None:
        if self.stream is not None:
            self.stream.close()
        if day != self.day:
            self.day = day
            self.part = 0
            prune(self.folder, day, self.keep_days)
            while log_path(self.folder, day, self.part + 1).exists():
                self.part += 1
        path = log_path(self.folder, day, self.part)
        self.stream = path.open("a", encoding="utf-8", newline="\n")
        self.size = path.stat().st_size

    def emit(self, record: logging.LogRecord) -> None:
        try:
            line = (self.format(record) + "\n").encode("utf-8")
            with self.guard:
                day = today_kst()
                if self.stream is None or day != self.day:
                    self._open(day)
                elif self.size + len(line) > self.max_bytes and self.size > 0:
                    self.part += 1
                    self._open(day)
                if self.stream is None:
                    return
                self.stream.write(line.decode("utf-8"))
                self.stream.flush()
                self.size += len(line)
        except Exception:
            self.handleError(record)

    def close(self) -> None:
        with self.guard:
            if self.stream is not None:
                self.stream.close()
                self.stream = None
        super().close()


def setup_logging(data_dir: str, level: int = logging.INFO) -> None:
    formatter = MaskingFormatter(FORMAT)
    handlers: list[logging.Handler] = [logging.StreamHandler(sys.stderr)]
    if data_dir:
        handlers.append(DailySizeHandler(Path(data_dir) / "logs"))
    root = logging.getLogger()
    for handler in list(root.handlers):
        root.removeHandler(handler)
    for handler in handlers:
        handler.setFormatter(formatter)
        root.addHandler(handler)
    root.setLevel(level)
