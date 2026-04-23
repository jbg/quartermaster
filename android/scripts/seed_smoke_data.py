#!/usr/bin/env python3
"""Seed repeatable local smoke data for the Android emulator flow.

This helper is intentionally local-only. It talks to the running Quartermaster
backend over HTTP, then uses the repo's default SQLite database to force one
expiry reminder due so the Android UI smoke can exercise the inbox path without
waiting for wall-clock reminder timing.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import sqlite3
import sys
import urllib.error
import urllib.request
import uuid
from pathlib import Path


DEFAULT_USERNAME = "android_smoke_18423"
DEFAULT_PASSWORD = "quartermaster-smoke-18423"
DEFAULT_EMAIL = "android-smoke@example.com"
DEFAULT_SERVER_URL = "http://127.0.0.1:8080"


class ApiError(RuntimeError):
    pass


class ApiClient:
    def __init__(self, base_url: str) -> None:
        self.base_url = base_url.rstrip("/")
        self.token: str | None = None

    def request(self, method: str, path: str, body: dict | None = None, *, allow_404: bool = False) -> dict | list | None:
        payload = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(
            self.base_url + path,
            data=payload,
            headers={"Accept": "application/json"},
            method=method,
        )
        if payload is not None:
            request.add_header("Content-Type", "application/json")
        if self.token:
            request.add_header("Authorization", f"Bearer {self.token}")

        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                raw = response.read().decode()
        except urllib.error.HTTPError as exc:
            if allow_404 and exc.code == 404:
                return None
            message = exc.read().decode()
            raise ApiError(f"{method} {path} failed with HTTP {exc.code}: {message}") from exc
        except urllib.error.URLError as exc:
            raise ApiError(f"{method} {path} failed: {exc}") from exc

        if not raw:
            return None
        return json.loads(raw)

    def healthcheck(self) -> None:
        result = self.request("GET", "/healthz")
        if not isinstance(result, dict) or result.get("status") != "ok":
            raise ApiError(f"unexpected health response: {result!r}")

    def login_or_register(self, username: str, password: str, email: str) -> None:
        try:
            result = self.request(
                "POST",
                "/auth/login",
                {"username": username, "password": password, "device_label": "Android Smoke Seed"},
            )
        except ApiError:
            result = self.request(
                "POST",
                "/auth/register",
                {
                    "username": username,
                    "password": password,
                    "email": email,
                    "invite_code": None,
                    "device_label": "Android Smoke Seed",
                },
            )
        if not isinstance(result, dict) or "access_token" not in result:
            raise ApiError(f"unexpected auth response: {result!r}")
        self.token = result["access_token"]


def ensure_household(client: ApiClient, timezone: str, household_name: str) -> dict:
    me = client.request("GET", "/auth/me")
    assert isinstance(me, dict)
    if me.get("current_household") is None:
        me = client.request("POST", "/households", {"name": household_name, "timezone": timezone})
        assert isinstance(me, dict)
    return me


def ensure_pantry(client: ApiClient) -> dict:
    locations = client.request("GET", "/locations")
    assert isinstance(locations, list)
    for location in locations:
        if location["kind"] == "pantry":
            return location
    created = client.request("POST", "/locations", {"name": "Pantry", "kind": "pantry"})
    assert isinstance(created, dict)
    return created


def create_product(client: ApiClient, name: str) -> dict:
    result = client.request(
        "POST",
        "/products",
        {
            "name": name,
            "brand": "Quartermaster",
            "family": "mass",
            "preferred_unit": "g",
            "barcode": None,
            "image_url": None,
        },
    )
    assert isinstance(result, dict)
    return result


def create_stock_batch(client: ApiClient, product_id: str, location_id: str, expires_on: str, note: str) -> dict:
    result = client.request(
        "POST",
        "/stock",
        {
            "location_id": location_id,
            "product_id": product_id,
            "quantity": "500",
            "unit": "g",
            "expires_on": expires_on,
            "opened_on": None,
            "note": note,
        },
    )
    assert isinstance(result, dict)
    return result


def create_invite(client: ApiClient) -> dict:
    result = client.request(
        "POST",
        "/households/current/invites",
        {
            "expires_at": "2999-01-01T00:00:00Z",
            "max_uses": 2,
            "role_granted": "member",
        },
    )
    assert isinstance(result, dict)
    return result


def force_due_reminder(
    db_path: Path,
    *,
    household_id: str,
    household_timezone: str,
    batch_id: str,
    product_id: str,
    product_name: str,
    location_id: str,
    location_name: str,
    expires_on: str,
) -> None:
    now = dt.datetime.now(dt.timezone.utc)
    fire_at = now.replace(hour=9, minute=0, second=0, microsecond=0).isoformat().replace("+00:00", "Z")
    created_at = now.isoformat(timespec="milliseconds").replace("+00:00", "Z")
    household_fire_local_at = now.isoformat(timespec="seconds").replace("+00:00", "+00:00")
    title = f"{product_name} expires tomorrow"
    body = location_name

    with sqlite3.connect(db_path) as connection:
        existing = connection.execute(
            "SELECT id FROM stock_reminder WHERE batch_id = ? AND kind = 'expiry' ORDER BY created_at DESC LIMIT 1",
            (batch_id,),
        ).fetchone()
        if existing:
            connection.execute(
                """
                UPDATE stock_reminder
                SET fire_at = ?,
                    title = ?,
                    body = ?,
                    household_timezone = ?,
                    expires_on = ?,
                    household_fire_local_at = ?,
                    acked_at = NULL
                WHERE id = ?
                """,
                (fire_at, title, body, household_timezone, expires_on, household_fire_local_at, existing[0]),
            )
        else:
            connection.execute(
                """
                INSERT INTO stock_reminder(
                    id, household_id, batch_id, product_id, location_id, kind, fire_at,
                    title, body, created_at, household_timezone, expires_on,
                    household_fire_local_at, acked_at
                ) VALUES (?, ?, ?, ?, ?, 'expiry', ?, ?, ?, ?, ?, ?, ?, NULL)
                """,
                (
                    str(uuid.uuid4()),
                    household_id,
                    batch_id,
                    product_id,
                    location_id,
                    fire_at,
                    title,
                    body,
                    created_at,
                    household_timezone,
                    expires_on,
                    household_fire_local_at,
                ),
            )
        connection.commit()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--server-url", default=DEFAULT_SERVER_URL)
    parser.add_argument("--username", default=DEFAULT_USERNAME)
    parser.add_argument("--password", default=DEFAULT_PASSWORD)
    parser.add_argument("--email", default=DEFAULT_EMAIL)
    parser.add_argument("--timezone", default="UTC")
    parser.add_argument("--household-name", default="My Household")
    parser.add_argument("--product-name", default="Smoke Rice")
    parser.add_argument(
        "--db-path",
        default=str(Path(__file__).resolve().parents[2] / "data.db"),
        help="path to the local SQLite database used by qm-server",
    )
    args = parser.parse_args()

    db_path = Path(args.db_path)
    if not db_path.exists():
        raise SystemExit(f"local SQLite database not found at {db_path}")

    client = ApiClient(args.server_url)
    client.healthcheck()
    client.login_or_register(args.username, args.password, args.email)
    me = ensure_household(client, args.timezone, args.household_name)
    assert isinstance(me["current_household"], dict)
    household = me["current_household"]
    pantry = ensure_pantry(client)
    product = create_product(client, args.product_name)
    expires_on = (dt.date.today() + dt.timedelta(days=1)).isoformat()
    batch = create_stock_batch(
        client,
        product_id=product["id"],
        location_id=pantry["id"],
        expires_on=expires_on,
        note="Android smoke seed",
    )
    invite = create_invite(client)
    force_due_reminder(
        db_path,
        household_id=household["id"],
        household_timezone=household["timezone"],
        batch_id=batch["id"],
        product_id=product["id"],
        product_name=product["name"],
        location_id=pantry["id"],
        location_name=pantry["name"],
        expires_on=expires_on,
    )

    print("Android smoke data ready")
    print(f"username={args.username}")
    print(f"password={args.password}")
    print(f"invite_code={invite['code']}")
    print(f"server_url={args.server_url}")
    print(f"device_server_url=http://127.0.0.1:8080")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ApiError as exc:
        raise SystemExit(str(exc)) from exc
