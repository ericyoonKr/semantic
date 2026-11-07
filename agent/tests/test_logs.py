import logging
import os
import time
from datetime import date, timedelta

from semantic_agent.logs import MASK, DailySizeHandler, MaskingFormatter, log_path, mask, prune, today_kst

MUST_MASK = [
    "900101-1234567", "9001011234567", "851231-2345678", "0203154123456", "020315-4123456",
    "010-1234-5678", "01012345678", "011-123-4567", "016-1234-5678", "019-987-6543",
    "02-123-4567", "02-1234-5678", "031-123-4567", "051-9876-5432", "0311234567",
    "070-1234-5678", "1588-1234-5678", "110-123-456789", "1002-123-456789", "123-456789-01-234",
    "3333-01-1234567", "352-1234-5678-93", "079-21-0123-456", "301-0123-4567-11", "94-1234567890",
    "1234567890", "12345678901", "123456789012", "1234567890123", "12345678901234",
]
MUST_KEEP = [
    "2026-11-08", "2026-11-08T13:45:10", "13:45:10,123", "2022두1234", "2023다12345",
    "123e4567-e89b-12d3-a456-426614174000", "12345678-1234-1234-1234-123456789012",
    "4f9a0c2d7e8b4a1c9d3e5f7a2b4c6d8e", "a1b2c3d4e5f60718293a4b5c6d7e8f90", "209715200",
    "localhost:8090", "8000", "45.0s", "300초", "제750조", "제5조의2", "R1_가짜요건",
    "L12", "근거 12", "status=502", "1.6.6", "v0.7.19", "2024. 1. 5.", "10,000,000원",
    "123456789", "0000가0000", "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
    "file_id=fake-file", "pages=12", "tokens=300000", "calls=40",
]


def test_masking_accuracy() -> None:
    misses = [s for s in MUST_MASK if MASK not in mask(f"x {s} y")]
    false_masks = [s for s in MUST_KEEP if mask(f"x {s} y") != f"x {s} y"]
    assert len(MUST_MASK) == 30 and len(MUST_KEEP) == 31
    assert misses == []
    assert false_masks == []


def test_formatter_masks_the_whole_line() -> None:
    record = logging.LogRecord("t", logging.INFO, __file__, 1, "call from %s id=%s", ("010-1234-5678", "rid-1"), None)
    line = MaskingFormatter("%(message)s").format(record)
    assert line == f"call from {MASK} id=rid-1"


def _record(text: str) -> logging.LogRecord:
    return logging.LogRecord("t", logging.INFO, __file__, 1, text, (), None)


def test_handler_writes_dated_file_and_rolls_by_size(tmp_path) -> None:
    handler = DailySizeHandler(tmp_path, max_bytes=200)
    handler.setFormatter(logging.Formatter("%(message)s"))
    for _ in range(6):
        handler.emit(_record("가" * 20))
    handler.close()
    today = today_kst()
    first, second = log_path(tmp_path, today, 0), log_path(tmp_path, today, 1)
    assert first.name == f"agent-{today.isoformat()}.log"
    assert second.name == f"agent-{today.isoformat()}.1.log"
    assert first.stat().st_size <= 200 and second.exists()
    assert sum(p.stat().st_size for p in tmp_path.iterdir()) == 6 * 61


def test_files_older_than_fourteen_days_are_removed(tmp_path) -> None:
    today = date(2026, 11, 8)
    keep = [log_path(tmp_path, today - timedelta(days=13), 0), log_path(tmp_path, today, 2)]
    drop = [log_path(tmp_path, today - timedelta(days=14), 0), log_path(tmp_path, today - timedelta(days=30), 1)]
    other = tmp_path / "gateway-2000-01-01.log"
    for path in [*keep, *drop, other]:
        path.write_text("x", encoding="utf-8")
    removed = prune(tmp_path, today)
    assert sorted(removed) == sorted(drop)
    assert all(p.exists() for p in [*keep, other])


def test_restart_continues_the_last_part(tmp_path) -> None:
    today = today_kst()
    log_path(tmp_path, today, 0).write_text("a\n", encoding="utf-8")
    log_path(tmp_path, today, 1).write_text("b\n", encoding="utf-8")
    handler = DailySizeHandler(tmp_path)
    handler.setFormatter(logging.Formatter("%(message)s"))
    handler.emit(_record("c"))
    handler.close()
    assert log_path(tmp_path, today, 1).read_text(encoding="utf-8") == "b\nc\n"
    assert os.path.getmtime(log_path(tmp_path, today, 1)) <= time.time()
