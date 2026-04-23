#!/usr/bin/env python3
"""Seed repeatable Android smoke data through the backend's internal fixture API."""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request


DEFAULT_SERVER_URL = "http://127.0.0.1:8080"


class ApiError(RuntimeError):
    pass


def request_fixture(server_url: str, maintenance_token: str) -> dict:
    request = urllib.request.Request(
        server_url.rstrip("/") + "/internal/maintenance/seed-android-smoke",
        data=b"",
        method="POST",
        headers={
            "Accept": "application/json",
            "X-QM-Maintenance-Token": maintenance_token,
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            payload = response.read().decode()
    except urllib.error.HTTPError as exc:
        raise ApiError(
            f"POST /internal/maintenance/seed-android-smoke failed with HTTP {exc.code}: {exc.read().decode()}"
        ) from exc
    except urllib.error.URLError as exc:
        raise ApiError(f"fixture request failed: {exc}") from exc

    data = json.loads(payload)
    if not isinstance(data, dict):
        raise ApiError(f"unexpected fixture response: {data!r}")
    return data


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--server-url", default=DEFAULT_SERVER_URL)
    parser.add_argument(
        "--maintenance-token",
        default=None,
        help="shared secret for /internal/maintenance/seed-android-smoke; defaults to QM_ANDROID_SMOKE_MAINTENANCE_TOKEN",
    )
    args = parser.parse_args()

    maintenance_token = args.maintenance_token or os.environ.get("QM_ANDROID_SMOKE_MAINTENANCE_TOKEN")
    if not maintenance_token:
        parser.error(
            "provide --maintenance-token or QM_ANDROID_SMOKE_MAINTENANCE_TOKEN for the backend smoke fixture route"
        )

    fixture = request_fixture(args.server_url, maintenance_token)
    print("Android smoke data ready")
    print(f"username={fixture['username']}")
    print(f"password={fixture['password']}")
    print(f"invite_code={fixture['invite_code']}")
    print(f"server_url={fixture['server_url']}")
    print(f"reminder_count={len(fixture.get('reminders', []))}")
    print(f"fixture_json={json.dumps(fixture, separators=(',', ':'))}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ApiError as exc:
        raise SystemExit(str(exc)) from exc
