"""Explicit MT5 terminal selection and data-directory pinning.

MetaTrader 5 stays a separately installed, official broker terminal.
Python MetaTrader5.initialize(path, portable=True) starts/connects to that
terminal when opted in. It must never quietly bind another installed MT5.
No account login, password or broker order APIs are called here.
"""
from __future__ import annotations

import os
from pathlib import Path


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
