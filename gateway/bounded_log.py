"""Bound owned Gateway stderr without renaming its open Windows handle.

Diagnostic text only: never opens or prunes an execution journal/database.
The previous bounded tail is replaced atomically before the active file is
truncated in place. Logging failures never trigger trading retries.
"""
from __future__ import annotations
import os
from pathlib import Path
import stat
import tempfile
import threading
import time

MAX_BYTES = 4 * 1024 * 1024


class BoundedStderr:
    def __init__(self, stream, path: Path, max_bytes=MAX_BYTES):
        if max_bytes < 256:
            raise ValueError('Diagnostic log limit must be at least 256 bytes')
        self.stream = stream
        self.path = Path(path)
        self.maximum = max_bytes
        self.lock = threading.RLock()
        self.retry_after = 0.0
        self.dropped_characters = 0
        self.fd = stream.fileno()
        if not self._owned_regular_file():
            raise ValueError('stderr is not the owned Gateway log')
        # Existing installations may already have a large diagnostic log.
        with self.lock:
            try:
                self._make_room(0)
            except OSError:
                self.retry_after = time.monotonic() + 1.0

    def _owned_regular_file(self):
        current = os.fstat(self.fd)
        if self.path.is_symlink() or not stat.S_ISREG(current.st_mode) or current.st_nlink != 1:
            return False
        actual = self.path.stat()
        return (current.st_dev, current.st_ino) == (actual.st_dev, actual.st_ino)

    def _make_room(self, incoming):
        self.stream.flush()
        size = os.fstat(self.fd).st_size
        if size + incoming <= self.maximum:
            return
        if not self._owned_regular_file():
            raise OSError('Diagnostic log identity changed')
        # The active path must remain open/in place on Windows. Only the
        # closed .1 snapshot is replaced; it is at most maximum bytes.
        temporary = None
        try:
            with self.path.open('rb') as source:
                source.seek(max(0, size - self.maximum))
                with tempfile.NamedTemporaryFile(
                        prefix='gateway-log-', suffix='.tmp',
                        dir=self.path.parent, delete=False) as target:
                    temporary = Path(target.name)
                    remaining = self.maximum
                    while remaining:
                        block = source.read(min(65536, remaining))
                        if not block:
                            break
                        target.write(block)
                        remaining -= len(block)
            os.replace(temporary, self.path.with_name(self.path.name + '.1'))
            temporary = None
            # Rust may pass a Windows append-only handle without truncate
            # rights. Reopen the SAME file with write access, verify identity,
            # then truncate in place; never rename the inherited active file.
            with self.path.open('r+b') as active:
                old = os.fstat(self.fd)
                new = os.fstat(active.fileno())
                if (old.st_dev, old.st_ino) != (new.st_dev, new.st_ino):
                    raise OSError('Diagnostic log identity changed')
                active.truncate(0)
            os.lseek(self.fd, 0, os.SEEK_END)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)

    def write(self, text):
        if not isinstance(text, str):
            raise TypeError('stderr.write requires str')
        with self.lock:
            if time.monotonic() < self.retry_after:
                self.dropped_characters += len(text)
                return len(text)
            offset = 0
            try:
                chunk_size = max(1, min(4096, self.maximum // 16))
                while offset < len(text):
                    chunk = text[offset:offset + chunk_size]
                    size = len(chunk.encode(self.encoding or 'utf-8', self.errors or 'backslashreplace'))
                    # TextIOWrapper may translate newlines on Windows.
                    if os.name == 'nt':
                        size += chunk.count('\n')
                    self._make_room(size)
                    self.stream.write(chunk)
                    self.stream.flush()
                    offset += len(chunk)
            except (OSError, ValueError):
                # Do not append indefinitely if archive/truncate fails.
                # A diagnostic disk problem must not resend or abort an order.
                self.dropped_characters += len(text) - offset
                self.retry_after = time.monotonic() + 1.0
        return len(text)

    def flush(self):
        with self.lock:
            try:
                self.stream.flush()
            except (OSError, ValueError):
                self.retry_after = time.monotonic() + 1.0

    def __getattr__(self, name):
        return getattr(self.stream, name)


def bounded_stderr(stream, path: Path):
    """Keep consoles, redirected tests and unowned paths completely unchanged."""
    try:
        return BoundedStderr(stream, path)
    except (OSError, ValueError, AttributeError):
        return stream
