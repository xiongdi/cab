#!/usr/bin/env python3
"""Restore provider keys and model enabled flags from ~/.cab/setting.json backup."""

from __future__ import annotations

import json
import sys
import urllib.error
import urllib.request
from pathlib import Path

CAB_DIR = Path.home() / ".cab"
SETTINGS_PATH = CAB_DIR / "settings.json"
BACKUP_PATH = CAB_DIR / "setting.json"
SETTINGS_BACKUP_PATH = CAB_DIR / "settings.json.bak"
BASE = "http://127.0.0.1:3125/api"


def request(method: str, path: str, key: str, body: dict | None = None):
    data = None if body is None else json.dumps(body).encode()
    headers = {"Authorization": f"Bearer {key}"}
    if body is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(f"{BASE}{path}", data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req) as resp:
            raw = resp.read()
            return resp.status, json.loads(raw) if raw else None
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"{method} {path} -> HTTP {exc.code}: {detail}") from exc


def configured_provider_count(settings: dict) -> int:
    count = 0
    for prov in (settings.get("providers") or {}).values():
        if provider_payload(prov):
            count += 1
    return count


def provider_payload(prov: dict) -> dict | None:
    api_keys = [
        k
        for k in prov.get("api_keys", [])
        if k.get("key", "").strip() and k.get("enabled", True)
    ]
    legacy_key = prov.get("api_key", "").strip()
    if not api_keys and not legacy_key:
        return None
    payload: dict = {
        "enabled": prov.get("enabled", True),
        "api_keys": api_keys
        or [{"key": legacy_key, "enabled": True, "subscribed": False}],
    }
    if prov.get("endpoints"):
        payload["endpoints"] = prov["endpoints"]
    return payload


def main() -> int:
    if not SETTINGS_PATH.exists():
        print(f"Missing {SETTINGS_PATH}", file=sys.stderr)
        return 1
    if not BACKUP_PATH.exists():
        print(f"Missing backup {BACKUP_PATH}", file=sys.stderr)
        return 1

    settings = json.loads(SETTINGS_PATH.read_text(encoding="utf-8"))
    backup = json.loads(BACKUP_PATH.read_text(encoding="utf-8"))
    key = settings["gateway_key"]

    # If settings.json was wiped but settings.json.bak still has keys, merge them in-memory.
    if configured_provider_count(settings) == 0 and SETTINGS_BACKUP_PATH.exists():
        try:
            bak = json.loads(SETTINGS_BACKUP_PATH.read_text(encoding="utf-8"))
            for provider_id, prov in (bak.get("providers") or {}).items():
                if provider_payload(prov):
                    settings.setdefault("providers", {})[provider_id] = prov
            if not settings.get("models") and bak.get("models"):
                settings["models"] = bak["models"]
            print(f"merged provider/model overrides from {SETTINGS_BACKUP_PATH.name}")
        except json.JSONDecodeError:
            print(f"warning: could not parse {SETTINGS_BACKUP_PATH}", file=sys.stderr)

    restored_providers: list[str] = []
    seen: set[str] = set()

    # Prefer live settings.json overrides when keys still exist.
    for provider_id, prov in (settings.get("providers") or {}).items():
        payload = provider_payload(prov)
        if not payload:
            continue
        request("PUT", f"/providers/{provider_id}", key, payload)
        restored_providers.append(provider_id)
        seen.add(provider_id)
        print(f"provider {provider_id}: restored from settings.json ({len(payload['api_keys'])} key(s))")

    # Fall back to legacy setting.json backup (array of provider records).
    for prov in backup.get("providers", []):
        provider_id = prov.get("id")
        if not provider_id or provider_id in seen:
            continue
        payload = provider_payload(prov)
        if not payload:
            continue
        request("PUT", f"/providers/{provider_id}", key, payload)
        restored_providers.append(provider_id)
        seen.add(provider_id)
        print(f"provider {provider_id}: restored from setting.json backup ({len(payload['api_keys'])} key(s))")

    if not restored_providers:
        print("no provider keys found to restore")

    enabled_by_provider: dict[str, set[str]] = {}
    for model in backup.get("models", []):
        if not model.get("enabled"):
            continue
        pid = model.get("provider_id")
        if pid:
            enabled_by_provider.setdefault(pid, set()).add(model["name"].lower())

    _, models = request("GET", "/models", key)
    enabled_count = 0
    for model in models:
        names = enabled_by_provider.get(model["provider_id"])
        if not names or model["name"].lower() not in names:
            continue
        request("PUT", f"/models/{model['id']}", key, {"enabled": True})
        enabled_count += 1
        print(f"model enabled: {model['name']}")

    _, routable = request("GET", "/models/routable", key)
    print(f"routable models: {len(routable)}")

    _, explain = request("POST", "/routing/explain", key, {"agent": "claude-code", "model": "auto"})
    resolved = explain.get("resolved")
    if resolved:
        print(f"auto routing: {resolved['model_id']} @ {resolved['provider_id']}")
    else:
        print("auto routing: unresolved (add opencode-go key or enable more models)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
