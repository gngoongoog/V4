"""Explicit MT5 terminal selection and data-directory pinning.

MetaTrader 5 stays a separately installed, official broker terminal.
Python MetaTrader5.initialize(path, portable=True) starts/connects to that
terminal when opted in. It must never quietly bind another installed MT5.
No account login, password or broker order APIs are called here.
"""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import tempfile


def canonical_directory(path):
    """Comparable Windows directory identity; no reliance on display case."""
    value = str(path or "").strip()
    if not value:
        return ""
    return os.path.normcase(os.path.realpath(os.path.abspath(value)))


class ManagedTerminalBinding:
    def __init__(self, executable="", portable=False):
        self.executable = str(executable or "").strip()
        self.portable = portable is True
        self.data_path = None
        self.expected_dir = (
            canonical_directory(Path(self.executable).parent) if self.executable else ""
        )
        self.last_error = ""
        self._owner_fd = None
        self._owner_path = None

    def acquire_ownership(self):
        """OS-held cross-process lock for this exact terminal data folder.

        Lock files reside in the per-user temporary folder, not inside the
        broker's MT5 install/data folders. Stale files are safe: OS releases
        locks on crash. Another BrainTrade Core cannot silently bind the same
        terminal even with a separate Core data directory.
        """
        if not self.data_path:
            self.last_error = "terminal_data_directory_unverified"
            return False
        if self._owner_fd is not None:
            return True
        scope = hashlib.sha256(self.data_path.encode("utf-8")).hexdigest()
        directory = Path(tempfile.gettempdir()) / "braintrade-mt5-owner-locks"
        fd = None
        try:
            directory.mkdir(mode=0o700, parents=True, exist_ok=True)
            lockpath = directory / (scope + ".lock")
            fd = os.open(lockpath, os.O_CREAT | os.O_RDWR, 0o600)
            if os.fstat(fd).st_size == 0:
                os.write(fd, b"0")
            os.lseek(fd, 0, os.SEEK_SET)
            if os.name == "nt":
                import msvcrt
                msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except (OSError, ImportError):
            if fd is not None:
                os.close(fd)
            self.last_error = "terminal_already_owned_or_lock_unavailable"
            return False
        self._owner_fd = fd
        self._owner_path = lockpath
        return True

    def release(self):
        fd, self._owner_fd = self._owner_fd, None
        self._owner_path = None
        if fd is None:
            return
        try:
            os.lseek(fd, 0, os.SEEK_SET)
            if os.name == "nt":
                import msvcrt
                msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
            else:
                import fcntl
                fcntl.flock(fd, fcntl.LOCK_UN)
        finally:
            os.close(fd)

    def validate(self):
        if self.portable and not self.executable:
            return False, "portable_requires_explicit_terminal_path"
        if not self.executable:
            return True, "legacy_auto_detection_not_pinned"
        exe = Path(self.executable)
        if not exe.is_absolute() or not exe.is_file() or exe.suffix.lower() != ".exe":
            return False, "terminal_path_invalid"
        if exe.name.lower() not in ("terminal64.exe", "terminal.exe", "metatrader64.exe"):
            return False, "terminal_executable_name_invalid"
        if self.portable and not os.access(exe.parent, os.W_OK):
            return False, "portable_terminal_directory_not_writable"
        return True, "pinned"

    def verify(self, terminal_info):
        if terminal_info is None:
            self.last_error = "terminal_information_unavailable"
            return False
        actual_dir = canonical_directory(getattr(terminal_info, "path", ""))
        actual_data = canonical_directory(getattr(terminal_info, "data_path", ""))
        if not actual_dir or not actual_data:
            self.last_error = "terminal_install_or_data_folder_unverified"
            return False
        if self.expected_dir and actual_dir != self.expected_dir:
            self.last_error = "connected_to_different_terminal_installation"
            return False
        if self.portable and actual_data != self.expected_dir:
            self.last_error = "portable_data_directory_does_not_match_installation"
            return False
        if self.data_path is not None and self.data_path != actual_data:
            self.last_error = "terminal_data_directory_changed"
            return False
        self.data_path = actual_data
        self.last_error = ""
        return True

    def connect(self, mt5, timeout_ms=10000):
        ok, reason = self.validate()
        if not ok:
            self.last_error = reason
            return False
        try:
            if self.executable:
                result = mt5.initialize(
                    self.executable, timeout=timeout_ms, portable=self.portable
                )
            else:
                # Compatibility for existing installations without a saved
                # MT5 path. The UI instructs owners to pin the executable.
                result = mt5.initialize(timeout=timeout_ms)
            if not result:
                self.last_error = "mt5_initialize_failed"
                return False
            if not self.verify(mt5.terminal_info()):
                # Disconnect only the Python API; do NOT kill the terminal.
                mt5.shutdown()
                return False
            if not self.acquire_ownership():
                # A second Core must not execute on the same MT5 terminal.
                mt5.shutdown()
                return False
            return True
        except (OSError, RuntimeError, ValueError, TypeError):
            self.last_error = "mt5_initialize_or_verify_exception"
            try:
                mt5.shutdown()
            except (OSError, RuntimeError):
                pass
            return False

    def check_current(self, mt5):
        try:
            return self.verify(mt5.terminal_info())
        except (OSError, RuntimeError, ValueError, TypeError):
            self.last_error = "terminal_identity_check_failed"
            return False
