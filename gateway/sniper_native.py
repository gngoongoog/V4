"""Windows-only native MT5 OnTick wake/feed for Sniper mode.

The MQL5 Expert Advisor sends authenticated quote-tick packets through a local
named pipe. The bridge keeps transport isolated from broker execution: the
existing Python gateway remains the only component allowed to submit orders.

BT2 adds a monotonic per-source tick event ID so multiple quote changes sharing
one MT5 `time_msc` remain distinct. BT1 is still accepted for an already
attached older bridge and maps to the first event slot of that millisecond.
"""
from __future__ import annotations

from collections import deque
import ctypes
import math
import os
from pathlib import Path
import secrets
import threading
import time

PIPE_NAME = r"\\.\pipe\BrainTradeSniperV1"
PROTOCOL = "BT2"
LEGACY_PROTOCOL = "BT1"
EVENT_FACTOR = 1_000_000
TOKEN_RELATIVE = Path("MQL5") / "Files" / "BrainTrade" / "sniper.token"
MAX_PACKET_BYTES = 512
MAX_PENDING_BYTES = 8192
DEFAULT_QUEUE_LIMIT = 8192


def clock_ms() -> int:
    return int(time.time() * 1000)


def base_event_id(time_msc: int) -> int:
    return int(time_msc) * EVENT_FACTOR


def session_token_path(data_path: str | os.PathLike[str]) -> Path:
    return Path(data_path) / TOKEN_RELATIVE


def write_session_token(data_path: str | os.PathLike[str], token: str) -> Path:
    """Atomically expose a short-lived token only inside this MT5 data folder."""
    path = session_token_path(data_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".token.tmp")
    temporary.write_text(token, encoding="ascii")
    try:
        os.chmod(temporary, 0o600)
    except OSError:
        pass
    temporary.replace(path)
    return path


def parse_packet(raw: bytes | str, token: str, received_ms: int | None = None) -> dict:
    """Validate BT2 or the backwards-compatible BT1 native quote packet."""
    if isinstance(raw, bytes):
        if len(raw) > MAX_PACKET_BYTES:
            raise ValueError("Native tick packet is too large")
        try:
            raw = raw.decode("ascii")
        except UnicodeDecodeError as exc:
            raise ValueError("Native tick packet must be ASCII") from exc
    try:
        encoded = raw.encode("ascii")
    except UnicodeEncodeError as exc:
        raise ValueError("Native tick packet must be ASCII") from exc
    if len(encoded) > MAX_PACKET_BYTES:
        raise ValueError("Native tick packet is too large")
    fields = raw.strip("\r\n").split("|")
    if not fields or fields[0] not in (PROTOCOL, LEGACY_PROTOCOL):
        raise ValueError("Native tick protocol mismatch")
    modern = fields[0] == PROTOCOL
    if (modern and len(fields) != 8) or (not modern and len(fields) != 7):
        raise ValueError("Native tick protocol mismatch")
    if not token or not secrets.compare_digest(fields[1], token):
        raise ValueError("Native tick session token mismatch")
    try:
        login = int(fields[2])
        time_msc = int(fields[4])
        if modern:
            event_id = int(fields[5])
            bid = float(fields[6])
            ask = float(fields[7])
        else:
            event_id = base_event_id(time_msc)
            bid = float(fields[5])
            ask = float(fields[6])
    except (TypeError, ValueError, OverflowError) as exc:
        raise ValueError("Native tick numeric field is invalid") from exc
    symbol = fields[3]
    if login <= 0 or time_msc <= 0:
        raise ValueError("Native tick identity/time is invalid")
    base = base_event_id(time_msc)
    if event_id < base or event_id >= base + EVENT_FACTOR:
        raise ValueError("Native tick event ID does not match quote millisecond")
    if not symbol or len(symbol) > 64 or any(ch in symbol for ch in "|\r\n\x00"):
        raise ValueError("Native tick symbol is invalid")
    if not all(math.isfinite(value) and value > 0 for value in (bid, ask)) or ask < bid:
        raise ValueError("Native tick quote is invalid")
    return {
        "login": login,
        "symbol": symbol,
        "time_msc": time_msc,
        "event_id": event_id,
        "bid": bid,
        "ask": ask,
        "received_ms": clock_ms() if received_ms is None else int(received_ms),
    }


class NativeTickMailbox:
    """Bounded FIFO. Overflow drops the oldest tick and records the loss."""
    def __init__(self, limit: int = DEFAULT_QUEUE_LIMIT):
        if limit < 1:
            raise ValueError("Native tick queue limit must be positive")
        self.limit = int(limit)
        self._items: deque[dict] = deque()
        self._lock = threading.Lock()
        self._wake = threading.Event()
        self.connected = False
        self.dropped = 0
        self.last_received_ms = 0

    def set_connected(self, value: bool) -> None:
        with self._lock:
            self.connected = bool(value)
        self._wake.set()

    def push(self, packet: dict) -> None:
        with self._lock:
            if len(self._items) >= self.limit:
                self._items.popleft()
                self.dropped += 1
            self._items.append(packet)
            self.last_received_ms = int(packet.get("received_ms", clock_ms()))
        self._wake.set()

    def drain(self, login: int, symbol: str, maximum: int = 256) -> list[dict]:
        selected: list[dict] = []
        maximum = max(1, int(maximum))
        with self._lock:
            while self._items and len(selected) < maximum:
                packet = self._items.popleft()
                if packet.get("login") == login and packet.get("symbol") == symbol:
                    selected.append(packet)
            if not self._items:
                self._wake.clear()
        return selected

    def recent(self, now_ms: int | None = None, maximum_age_ms: int = 500) -> bool:
        now_ms = clock_ms() if now_ms is None else int(now_ms)
        with self._lock:
            return self.connected and self.last_received_ms > 0 and 0 <= now_ms - self.last_received_ms <= maximum_age_ms

    def wait(self, timeout: float) -> None:
        self._wake.wait(max(0.0, float(timeout)))

    def wake(self) -> None:
        self._wake.set()


class _WindowsPipeServer:
    PIPE_ACCESS_DUPLEX = 0x00000003
    PIPE_TYPE_BYTE = 0x00000000
    PIPE_READMODE_BYTE = 0x00000000
    PIPE_WAIT = 0x00000000
    PIPE_REJECT_REMOTE_CLIENTS = 0x00000008
    ERROR_PIPE_CONNECTED = 535
    INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value

    def __init__(self, token: str, mailbox: NativeTickMailbox, pipe_name: str = PIPE_NAME):
        self.token = token
        self.mailbox = mailbox
        self.pipe_name = pipe_name
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if os.name != "nt" or self._thread is not None:
            return
        self._thread = threading.Thread(target=self._run, name="braintrade-sniper-pipe", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        self.mailbox.wake()
        self.mailbox.set_connected(False)
        # A synchronous ConnectNamedPipe may be waiting for MT5. The thread is
        # daemonized so process shutdown remains bounded even with no client.

    def _kernel(self):
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateNamedPipeW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                            wintypes.DWORD, wintypes.DWORD, wintypes.DWORD,
                                            wintypes.DWORD, ctypes.c_void_p]
        kernel.CreateNamedPipeW.restype = wintypes.HANDLE
        kernel.ConnectNamedPipe.argtypes = [wintypes.HANDLE, ctypes.c_void_p]
        kernel.ConnectNamedPipe.restype = wintypes.BOOL
        kernel.ReadFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD,
                                    ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
        kernel.ReadFile.restype = wintypes.BOOL
        kernel.DisconnectNamedPipe.argtypes = [wintypes.HANDLE]
        kernel.DisconnectNamedPipe.restype = wintypes.BOOL
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.CloseHandle.restype = wintypes.BOOL
        return kernel, wintypes

    def _run(self) -> None:
        kernel, wintypes = self._kernel()
        mode = self.PIPE_TYPE_BYTE | self.PIPE_READMODE_BYTE | self.PIPE_WAIT | self.PIPE_REJECT_REMOTE_CLIENTS
        while not self._stop.is_set():
            handle = kernel.CreateNamedPipeW(self.pipe_name, self.PIPE_ACCESS_DUPLEX, mode,
                                             1, 4096, 4096, 0, None)
            if handle == self.INVALID_HANDLE_VALUE:
                time.sleep(0.25)
                continue
            try:
                connected = bool(kernel.ConnectNamedPipe(handle, None))
                if not connected and ctypes.get_last_error() != self.ERROR_PIPE_CONNECTED:
                    continue
                self.mailbox.set_connected(True)
                pending = bytearray()
                while not self._stop.is_set():
                    buffer = ctypes.create_string_buffer(4096)
                    count = wintypes.DWORD()
                    if not kernel.ReadFile(handle, buffer, len(buffer), ctypes.byref(count), None):
                        break
                    if count.value == 0:
                        continue
                    pending.extend(buffer.raw[:count.value])
                    if len(pending) > MAX_PENDING_BYTES:
                        pending.clear()
                        continue
                    while b"\n" in pending:
                        line, _, remainder = pending.partition(b"\n")
                        pending = bytearray(remainder)
                        try:
                            self.mailbox.push(parse_packet(line, self.token))
                        except ValueError:
                            continue
            finally:
                self.mailbox.set_connected(False)
                try:
                    kernel.DisconnectNamedPipe(handle)
                except Exception:
                    pass
                kernel.CloseHandle(handle)


class NativeTickBridge:
    """Lifecycle wrapper bound to the currently selected MT5 data directory."""
    def __init__(self, queue_limit: int = DEFAULT_QUEUE_LIMIT):
        self.mailbox = NativeTickMailbox(queue_limit)
        self.token = ""
        self.token_path: Path | None = None
        self.data_path: Path | None = None
        self.server: _WindowsPipeServer | None = None

    @property
    def available(self) -> bool:
        return os.name == "nt" and self.server is not None

    @property
    def connected(self) -> bool:
        return self.mailbox.connected

    def bind_terminal(self, terminal_info) -> bool:
        if os.name != "nt" or terminal_info is None:
            return False
        data_path = Path(str(getattr(terminal_info, "data_path", "") or ""))
        if not str(data_path) or str(data_path) == ".":
            return False
        if self.server is not None and self.data_path == data_path:
            return True
        self.close()
        self.data_path = data_path
        self.token = secrets.token_urlsafe(32)
        self.token_path = write_session_token(data_path, self.token)
        self.server = _WindowsPipeServer(self.token, self.mailbox)
        self.server.start()
        return True

    def drain(self, login: int, symbol: str, maximum: int = 256) -> list[dict]:
        return self.mailbox.drain(int(login), symbol, maximum)

    def recent(self, now_ms: int | None = None, maximum_age_ms: int = 500) -> bool:
        return self.mailbox.recent(now_ms, maximum_age_ms)

    def wait(self, timeout: float) -> None:
        self.mailbox.wait(timeout)

    def close(self) -> None:
        server, token_path, token = self.server, self.token_path, self.token
        self.server = None
        self.token_path = None
        self.data_path = None
        self.token = ""
        if server is not None:
            server.stop()
        if token_path is not None:
            try:
                if token_path.is_file() and token_path.read_text("ascii").strip() == token:
                    token_path.unlink()
            except OSError:
                pass
