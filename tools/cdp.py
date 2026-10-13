"""Minimal Chrome DevTools Protocol client (pure stdlib).

Lets a test harness drive the app's WebView2: set localStorage, reload, and read
state back. Only what is needed is implemented — client-to-server frames are
masked per RFC 6455, server frames are read back until FIN.
"""
from __future__ import annotations

import asyncio
import base64
import json
import os
import struct
import sys
import urllib.request


def http_json(url: str, timeout: float = 10.0):
    with urllib.request.urlopen(url, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def find_page(port: int, tries: int = 40):
    """Wait for the debugger to come up and return the app page target."""
    last = None
    for _ in range(tries):
        try:
            targets = http_json(f"http://127.0.0.1:{port}/json")
            pages = [t for t in targets if t.get("type") == "page" and t.get("webSocketDebuggerUrl")]
            if pages:
                return pages[0]
        except Exception as exc:  # noqa: BLE001 - the port may not be open yet
            last = exc
        import time
        time.sleep(0.5)
    raise RuntimeError(f"no page target on port {port} (last error: {last})")


class WS:
    def __init__(self, ws_url: str):
        self.ws_url = ws_url
        self.reader: asyncio.StreamReader | None = None
        self.writer: asyncio.StreamWriter | None = None
        self.next_id = 1

    async def connect(self):
        url = self.ws_url
        assert url.startswith("ws://"), url
        rest = url[len("ws://"):]
        hostport, _, path = rest.partition("/")
        host, _, port = hostport.partition(":")
        port = int(port or 80)
        self.reader, self.writer = await asyncio.open_connection(host, port)
        key = base64.b64encode(os.urandom(16)).decode()
        req = (
            f"GET /{path} HTTP/1.1\r\n"
            f"Host: {hostport}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n"
            "Origin: http://127.0.0.1\r\n"
            "\r\n"
        )
        self.writer.write(req.encode())
        await self.writer.drain()
        # Read the handshake response headers.
        while True:
            line = await self.reader.readline()
            if line in (b"\r\n", b"\n", b""):
                break

    async def _send(self, payload: str):
        data = payload.encode("utf-8")
        header = bytearray([0x81])  # FIN + text
        mask_bit = 0x80
        length = len(data)
        if length < 126:
            header.append(mask_bit | length)
        elif length < (1 << 16):
            header.append(mask_bit | 126)
            header += struct.pack(">H", length)
        else:
            header.append(mask_bit | 127)
            header += struct.pack(">Q", length)
        mask = os.urandom(4)
        header += mask
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
        self.writer.write(bytes(header) + masked)
        await self.writer.drain()

    async def _read_frame(self) -> str:
        hdr = await self.reader.readexactly(2)
        length = hdr[1] & 0x7F
        if length == 126:
            length = struct.unpack(">H", await self.reader.readexactly(2))[0]
        elif length == 127:
            length = struct.unpack(">Q", await self.reader.readexactly(8))[0]
        payload = await self.reader.readexactly(length) if length else b""
        return payload.decode("utf-8", "replace")

    async def call(self, method: str, params: dict | None = None, timeout: float = 15.0) -> dict:
        msg_id = self.next_id
        self.next_id += 1
        await self._send(json.dumps({"id": msg_id, "method": method, "params": params or {}}))
        loop = asyncio.get_running_loop()
        deadline = loop.time() + timeout
        while True:
            remaining = deadline - loop.time()
            if remaining <= 0:
                # A navigation or reload can swallow the reply; callers that do
                # not need one must not block forever.
                raise TimeoutError(f"{method} did not answer within {timeout}s")
            raw = await asyncio.wait_for(self._read_frame(), timeout=remaining)
            try:
                msg = json.loads(raw)
            except json.JSONDecodeError:
                continue
            if msg.get("id") == msg_id:
                if "error" in msg:
                    raise RuntimeError(f"{method} failed: {msg['error']}")
                return msg.get("result", {})

    async def evaluate(self, expression: str):
        res = await self.call(
            "Runtime.evaluate",
            {"expression": expression, "returnByValue": True, "awaitPromise": True},
        )
        result = res.get("result", {})
        if res.get("exceptionDetails"):
            raise RuntimeError(f"evaluate threw: {res['exceptionDetails']}")
        return result.get("value")

    async def close(self):
        if self.writer:
            self.writer.close()


async def apply_and_reload(port: int, theme: str, ui: str):
    """Persist the selection in the app's own origin, then reload it."""
    page = find_page(port)
    ws = WS(page["webSocketDebuggerUrl"])
    await ws.connect()
    try:
        # Write first and confirm the write landed before reloading.
        await ws.evaluate(
            "(() => {"
            f"localStorage.setItem('kilo-app-theme', {json.dumps(theme)});"
            f"localStorage.setItem('kilo-app-ui-mode', {json.dumps(ui)});"
            "return true; })()"
        )
        stored = await ws.evaluate(
            "localStorage.getItem('kilo-app-theme') + '/' + localStorage.getItem('kilo-app-ui-mode')"
        )
        await ws.call("Page.reload", {"ignoreCache": True})
    finally:
        await ws.close()

    # Reconnect after the reload; the old target/context is gone.
    await asyncio.sleep(3.5)
    page = find_page(port)
    ws = WS(page["webSocketDebuggerUrl"])
    await ws.connect()
    try:
        applied = await ws.evaluate(
            "document.documentElement.getAttribute('data-ui-mode') + '/' +"
            " document.documentElement.getAttribute('data-theme')"
        )
        return stored, applied
    finally:
        await ws.close()


if __name__ == "__main__":
    port = int(sys.argv[1])
    theme = sys.argv[2] if len(sys.argv) > 2 else "dark"
    ui = sys.argv[3] if len(sys.argv) > 3 else "polished"
    before, after = asyncio.run(apply_and_reload(port, theme, ui))
    print(f"set -> {before}   after reload: {after}")
