#!/usr/bin/env python3
"""Minimal OpenAI/Anthropic mock that logs incoming request headers to a file."""
from __future__ import annotations

import json
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse

OUT_FILE = os.environ.get("CAB_CAPTURE_FILE", "/tmp/cab-header-capture.jsonl")


def cab_extract_agent(headers: dict[str, str]) -> str:
    normalized = {
        str(k).lower(): str(v)
        for k, v in headers.items()
    }
    cab_agent = normalized.get("x-cab-agent", "").strip().lower()
    aliases = {
        "claude": "claude-code",
        "kilo": "kilocode",
        "kilo-code": "kilocode",
    }
    supported = {
        "claude-code",
        "codex",
        "opencode",
        "hermes",
        "kilocode",
        "openclaw",
        "pi",
    }
    if cab_agent in supported:
        return cab_agent
    if cab_agent in aliases:
        return aliases[cab_agent]

    originator = normalized.get("originator", "").lower()
    if "codex" in originator:
        return "codex"

    ua = normalized.get("user-agent", "").lower()
    if "kilo-code" in ua or "kilocode" in ua:
        return "kilocode"
    if "openclaw" in ua:
        return "openclaw"
    if "pi-coding-agent" in ua or "pi-coding" in ua:
        return "pi"
    if "hermesagent" in ua or "hermes/" in ua:
        return "hermes"
    if "opencode/" in ua or ua.startswith("opencode"):
        return "opencode"
    if "codex" in ua:
        return "codex"
    if "claude" in ua:
        return "claude-code"
    return "unknown"


class Handler(BaseHTTPRequestHandler):
    client_tag: str = "unknown"

    def log_message(self, fmt: str, *args) -> None:  # noqa: ARG002
        return

    def _read_body(self) -> bytes:
        length = int(self.headers.get("Content-Length", "0"))
        return self.rfile.read(length) if length else b""

    def _capture(self, body: bytes) -> None:
        headers = {k: v for k, v in self.headers.items()}
        entry = {
            "client_tag": self.client_tag,
            "path": self.path,
            "method": self.command,
            "headers": headers,
            "cab_extract_agent": cab_extract_agent(headers),
            "body_preview": body[:500].decode("utf-8", errors="replace"),
        }
        with open(OUT_FILE, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(entry, ensure_ascii=False) + "\n")

    def _json(self, status: int, payload: dict) -> None:
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self) -> None:  # noqa: N802
        body = self._read_body()
        self._capture(body)
        path = urlparse(self.path).path

        if path.endswith("/messages"):
            self._json(
                200,
                {
                    "id": "msg_test",
                    "type": "message",
                    "role": "assistant",
                    "model": "claude-test",
                    "content": [{"type": "text", "text": "ok"}],
                    "stop_reason": "end_turn",
                    "usage": {"input_tokens": 1, "output_tokens": 1},
                },
            )
            return

        if path.endswith("/responses"):
            self._json(
                200,
                {
                    "id": "resp_test",
                    "object": "response",
                    "status": "completed",
                    "output": [
                        {
                            "type": "message",
                            "role": "assistant",
                            "content": [{"type": "output_text", "text": "ok"}],
                        }
                    ],
                },
            )
            return

        self._json(
            200,
            {
                "id": "chatcmpl_test",
                "object": "chat.completion",
                "model": "test",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "stop",
                    }
                ],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
            },
        )


def main() -> None:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    if os.path.exists(OUT_FILE):
        os.remove(OUT_FILE)
    server = HTTPServer(("127.0.0.1", port), Handler)
    print(json.dumps({"port": server.server_address[1], "file": OUT_FILE}), flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
