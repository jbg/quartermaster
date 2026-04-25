#!/usr/bin/env python3
"""Drive a minimal Android emulator smoke test via UIAutomator selectors.

This intentionally avoids hard-coded pixel coordinates. It reads the Android
accessibility tree, finds controls by visible text/label, and taps the center of
the matched node bounds.
"""

from __future__ import annotations

import argparse
import json
import os
import urllib.error
import urllib.parse
import urllib.request
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from typing import Iterable


PACKAGE = "dev.quartermaster.android"
ACTIVITY = f"{PACKAGE}/.MainActivity"
BOUNDS_RE = re.compile(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]")
INVITE_CODE_RE = re.compile(r"^[A-Z0-9]{8}$")
EXTRA_REMINDER_ID = "quartermaster.reminder_id"
EXTRA_BATCH_ID = "quartermaster.batch_id"
EXTRA_PRODUCT_ID = "quartermaster.product_id"
EXTRA_LOCATION_ID = "quartermaster.location_id"
EXTRA_KIND = "quartermaster.kind"
EXTRA_TITLE = "quartermaster.title"
EXTRA_BODY = "quartermaster.body"


@dataclass(frozen=True)
class UiNode:
    element: ET.Element
    parent: "UiNode | None"

    @property
    def text(self) -> str:
        return self.element.attrib.get("text", "")

    @property
    def klass(self) -> str:
        return self.element.attrib.get("class", "")

    @property
    def clickable(self) -> bool:
        return self.element.attrib.get("clickable") == "true"

    @property
    def resource_id(self) -> str:
        return self.element.attrib.get("resource-id", "")

    @property
    def bounds(self) -> tuple[int, int, int, int]:
        raw = self.element.attrib.get("bounds", "")
        match = BOUNDS_RE.fullmatch(raw)
        if not match:
            raise RuntimeError(f"node has invalid bounds: {raw!r}")
        return tuple(int(part) for part in match.groups())  # type: ignore[return-value]

    @property
    def center(self) -> tuple[int, int]:
        left, top, right, bottom = self.bounds
        return ((left + right) // 2, (top + bottom) // 2)

    def ancestors(self) -> Iterable["UiNode"]:
        node = self.parent
        while node is not None:
            yield node
            node = node.parent


def adb(*args: str, capture: bool = False) -> str:
    command = ["adb", *args]
    if capture:
        return subprocess.check_output(command, text=True)
    subprocess.check_call(command)
    return ""


def walk(element: ET.Element, parent: UiNode | None = None) -> Iterable[UiNode]:
    node = UiNode(element, parent)
    yield node
    for child in element:
        yield from walk(child, node)


def dump_nodes() -> list[UiNode]:
    adb("shell", "uiautomator", "dump", "/sdcard/window.xml")
    xml = adb("exec-out", "cat", "/sdcard/window.xml", capture=True)
    return list(walk(ET.fromstring(xml)))


def wait_for_text(text: str, timeout: float = 10.0) -> UiNode:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for node in dump_nodes():
            if node.text == text:
                return node
        time.sleep(0.25)
    raise RuntimeError(f"timed out waiting for text {text!r}")


def node_has_tag(node: UiNode, tag: str) -> bool:
    resource_id = node.resource_id
    return resource_id == tag or resource_id.endswith(f"/{tag}")


def wait_for_tag(tag: str, timeout: float = 10.0) -> UiNode:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for node in dump_nodes():
            if node_has_tag(node, tag):
                return node
        time.sleep(0.25)
    raise RuntimeError(f"timed out waiting for tag {tag!r}")


def wait_for_condition(description: str, predicate, timeout: float = 10.0):
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            value = predicate()
            if value:
                return value
        except Exception as exc:  # pragma: no cover - best effort in smoke script
            last_error = exc
        time.sleep(0.25)
    if last_error is not None:
        raise RuntimeError(f"timed out waiting for {description}: {last_error}") from last_error
    raise RuntimeError(f"timed out waiting for {description}")


def wait_for_text_with_scroll(text: str, attempts: int = 6) -> UiNode:
    for _ in range(attempts):
        for node in dump_nodes():
            if node.text == text:
                return node
        adb("shell", "input", "swipe", "540", "1900", "540", "900", "250")
        time.sleep(0.5)
    raise RuntimeError(f"timed out waiting for text {text!r} after scrolling")


def find_clickables_with_text(text: str) -> list[UiNode]:
    matches: list[UiNode] = []
    for node in dump_nodes():
        if node.text != text:
            continue
        if node.clickable:
            matches.append(node)
            continue
        for ancestor in node.ancestors():
            if ancestor.clickable:
                matches.append(ancestor)
                break
    deduped: dict[tuple[int, int, int, int], UiNode] = {}
    for match in matches:
        deduped[match.bounds] = match
    return list(deduped.values())


def find_clickables_with_tag(tag: str) -> list[UiNode]:
    matches: list[UiNode] = []
    for node in dump_nodes():
        if not node_has_tag(node, tag):
            continue
        if node.clickable:
            matches.append(node)
            continue
        for ancestor in node.ancestors():
            if ancestor.clickable:
                matches.append(ancestor)
                break
    deduped: dict[tuple[int, int, int, int], UiNode] = {}
    for match in matches:
        deduped[match.bounds] = match
    return list(deduped.values())


def find_clickables_with_tag_prefix(prefix: str) -> list[UiNode]:
    matches: list[UiNode] = []
    for node in dump_nodes():
        resource_id = node.resource_id
        if not (
            resource_id == prefix
            or resource_id.endswith(f"/{prefix}")
            or resource_id.endswith(prefix)
        ):
            continue
        if node.clickable:
            matches.append(node)
            continue
        for ancestor in node.ancestors():
            if ancestor.clickable:
                matches.append(ancestor)
                break
    deduped: dict[tuple[int, int, int, int], UiNode] = {}
    for match in matches:
        deduped[match.bounds] = match
    return list(deduped.values())


def find_clickable_with_text(text: str, *, lowest: bool = False) -> UiNode:
    matches = find_clickables_with_text(text)
    if not matches:
        raise RuntimeError(f"no clickable node found for text {text!r}")
    if lowest:
        return max(matches, key=lambda node: node.bounds[1])
    return min(matches, key=lambda node: node.bounds[1])


def find_clickable_with_tag(tag: str, *, lowest: bool = False) -> UiNode:
    matches = find_clickables_with_tag(tag)
    if not matches:
        raise RuntimeError(f"no clickable node found for tag {tag!r}")
    if lowest:
        return max(matches, key=lambda node: node.bounds[1])
    return min(matches, key=lambda node: node.bounds[1])


def find_clickable_with_tag_prefix(prefix: str) -> UiNode:
    matches = sorted(find_clickables_with_tag_prefix(prefix), key=lambda node: node.bounds[1])
    if not matches:
        raise RuntimeError(f"no clickable node found for tag prefix {prefix!r}")
    return matches[0]


def find_nth_clickable_with_text(text: str, index: int) -> UiNode:
    matches = sorted(find_clickables_with_text(text), key=lambda node: node.bounds[1])
    if index >= len(matches):
        raise RuntimeError(f"expected at least {index + 1} clickable nodes for text {text!r}, found {len(matches)}")
    return matches[index]


def find_edit_text_by_label(label: str) -> UiNode:
    for node in dump_nodes():
        if node.text != label:
            continue
        for ancestor in node.ancestors():
            if ancestor.klass == "android.widget.EditText":
                return ancestor
    raise RuntimeError(f"no EditText found for label {label!r}")


def find_edit_text_by_tag(tag: str) -> UiNode:
    for node in dump_nodes():
        if node_has_tag(node, tag):
            return node
    raise RuntimeError(f"no EditText found for tag {tag!r}")


def tap(node: UiNode) -> None:
    x, y = node.center
    print(f"tap {node.klass} text={node.text!r} bounds={node.element.attrib.get('bounds')} center=({x},{y})")
    adb("shell", "input", "tap", str(x), str(y))


def tap_text(text: str, *, lowest: bool = False) -> None:
    tap(find_clickable_with_text(text, lowest=lowest))


def tap_tag(tag: str, *, lowest: bool = False) -> None:
    tap(find_clickable_with_tag(tag, lowest=lowest))


def tap_first_tag_prefix(prefix: str) -> None:
    tap(find_clickable_with_tag_prefix(prefix))


def tap_nth_text(text: str, index: int) -> None:
    tap(find_nth_clickable_with_text(text, index))


def set_text_field(label: str, value: str) -> None:
    tap(find_edit_text_by_label(label))
    # The script starts from a cleared app install, so fields are empty except
    # Server URL, which this smoke path intentionally leaves at its default.
    adb("shell", "input", "text", value.replace(" ", "%s"))


def replace_text_field(label: str, value: str) -> None:
    field = find_edit_text_by_label(label)
    current = field.text
    tap(field)
    adb("shell", "input", "keyevent", "MOVE_END")
    for _ in range(len(current) + 4):
        adb("shell", "input", "keyevent", "DEL")
    adb("shell", "input", "text", value.replace(" ", "%s"))


def replace_text_field_by_tag(tag: str, value: str) -> None:
    field = find_edit_text_by_tag(tag)
    current = field.text
    tap(field)
    adb("shell", "input", "keyevent", "MOVE_END")
    for _ in range(len(current) + 4):
        adb("shell", "input", "keyevent", "DEL")
    adb("shell", "input", "text", value.replace(" ", "%s"))


def assert_text(text: str) -> None:
    wait_for_text(text)


def assert_text_missing(text: str, timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not any(node.text == text for node in dump_nodes()):
            return
        time.sleep(0.25)
    raise RuntimeError(f"text {text!r} still present after {timeout} seconds")


def assert_tag_missing(tag: str, timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not any(node_has_tag(node, tag) for node in dump_nodes()):
            return
        time.sleep(0.25)
    raise RuntimeError(f"tag {tag!r} still present after {timeout} seconds")


def count_clickables(text: str) -> int:
    return len(find_clickables_with_text(text))


def wait_for_clickable_count(text: str, expected: int, timeout: float = 10.0) -> None:
    wait_for_condition(
        f"{expected} clickable nodes for {text!r}",
        lambda: count_clickables(text) == expected,
        timeout=timeout,
    )


def first_text_matching(pattern: re.Pattern[str]) -> str | None:
    for node in dump_nodes():
        if pattern.fullmatch(node.text):
            return node.text
    return None


def wait_for_matching_text(pattern: re.Pattern[str], timeout: float = 10.0) -> str:
    return wait_for_condition(
        f"text matching {pattern.pattern}",
        lambda: first_text_matching(pattern),
        timeout=timeout,
    )


def launch(clear_app: bool) -> None:
    if clear_app:
        adb("shell", "pm", "clear", PACKAGE)
    adb("shell", "am", "start", "-n", ACTIVITY)


def open_invite_link(invite_code: str, server_url: str) -> None:
    encoded_server = urllib.parse.quote(server_url, safe="")
    deep_link = f"quartermaster://join?invite={invite_code}&server={encoded_server}"
    adb("shell", "am", "start", "-a", "android.intent.action.VIEW", "-d", deep_link)


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
        raise RuntimeError(
            f"fixture request failed with HTTP {exc.code}: {exc.read().decode()}"
        ) from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"fixture request failed: {exc}") from exc
    data = json.loads(payload)
    if not isinstance(data, dict):
        raise RuntimeError(f"unexpected fixture response: {data!r}")
    return data


def open_reminder_payload(payload: dict) -> None:
    adb(
        "shell",
        "am",
        "start",
        "-n",
        ACTIVITY,
        "--es",
        EXTRA_REMINDER_ID,
        payload["reminder_id"],
        "--es",
        EXTRA_BATCH_ID,
        payload["batch_id"],
        "--es",
        EXTRA_PRODUCT_ID,
        payload["product_id"],
        "--es",
        EXTRA_LOCATION_ID,
        payload["location_id"],
        "--es",
        EXTRA_KIND,
        payload["kind"],
        "--es",
        EXTRA_TITLE,
        payload["title"],
        "--es",
        EXTRA_BODY,
        payload["body"],
    )


def sign_in(username: str, password: str, server_url: str | None = None) -> None:
    wait_for_tag("smoke-onboarding-screen")
    if server_url is not None:
        replace_text_field_by_tag("smoke-server-url-field", server_url)
    replace_text_field_by_tag("smoke-username-field", username)
    replace_text_field_by_tag("smoke-password-field", password)
    adb("shell", "input", "keyevent", "BACK")
    time.sleep(1.0)
    tap_tag("smoke-sign-in-button")
    wait_for_tag("smoke-inventory-screen")


def exercise_products(fixture: dict | None) -> None:
    product_name = f"Android Smoke Product {int(time.time())}"
    updated_brand = "Smoke Brand Updated"

    tap_tag("smoke-tab-products")
    wait_for_tag("smoke-products-screen")
    tap_tag("smoke-product-create-button")
    wait_for_text("New product")
    replace_text_field_by_tag("smoke-product-name-field", product_name)
    replace_text_field_by_tag("smoke-product-brand-field", "Smoke Brand")
    tap_tag("smoke-product-submit-button")
    wait_for_text(product_name, timeout=15.0)

    tap_tag("smoke-product-edit-button")
    wait_for_text("Edit product")
    replace_text_field_by_tag("smoke-product-brand-field", updated_brand)
    tap_tag("smoke-product-submit-button")
    wait_for_text(updated_brand, timeout=15.0)

    tap_tag("smoke-product-delete-button")
    wait_for_text("Delete product")
    tap_tag("smoke-product-delete-confirm-button")
    wait_for_tag("smoke-product-list", timeout=15.0)
    assert_text_missing(product_name, timeout=5.0)

    tap_tag("smoke-product-filter-deleted")
    tap_tag("smoke-product-search-button")
    wait_for_text(product_name, timeout=15.0)
    tap_text(product_name)
    wait_for_tag("smoke-product-restore-button")
    tap_tag("smoke-product-restore-button")
    wait_for_tag("smoke-product-edit-button", timeout=15.0)

    if fixture is not None:
        tap_text("Back to products")
        wait_for_tag("smoke-product-list")
        replace_text_field_by_tag("smoke-product-barcode-field", fixture["barcode"])
        tap_tag("smoke-product-barcode-button")
        wait_for_text("Retry Beans", timeout=15.0)
        wait_for_tag("smoke-product-refresh-button")


def check_backend_health(server_url: str) -> None:
    health_url = server_url.rstrip("/") + "/healthz"
    try:
        with urllib.request.urlopen(health_url, timeout=3) as response:
            if response.status != 200:
                raise RuntimeError(f"{health_url} returned HTTP {response.status}")
    except (OSError, urllib.error.URLError) as exc:
        raise RuntimeError(f"backend health check failed for {health_url}: {exc}") from exc


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--username", default=os.environ.get("QM_ANDROID_SMOKE_USERNAME"))
    parser.add_argument("--password", default=os.environ.get("QM_ANDROID_SMOKE_PASSWORD"))
    parser.add_argument(
        "--maintenance-token",
        default=os.environ.get("QM_ANDROID_SMOKE_MAINTENANCE_TOKEN"),
        help="shared secret for /internal/maintenance/seed-android-smoke",
    )
    parser.add_argument(
        "--host-server-url",
        default=os.environ.get("QM_ANDROID_SMOKE_HOST_SERVER_URL", "http://127.0.0.1:8080"),
        help="host-side URL used for the preflight health check",
    )
    parser.add_argument(
        "--device-server-url",
        default=os.environ.get("QM_ANDROID_SMOKE_DEVICE_SERVER_URL", "http://127.0.0.1:8080"),
        help="server URL written into the Android app during the smoke run",
    )
    parser.add_argument(
        "--preserve-app-data",
        action="store_true",
        help="do not clear app data before launching",
    )
    args = parser.parse_args()

    check_backend_health(args.host_server_url)
    fixture = None
    if args.maintenance_token:
        fixture = request_fixture(args.host_server_url, args.maintenance_token)
        args.username = fixture["username"]
        args.password = fixture["password"]
    if not args.username or not args.password:
        parser.error(
            "provide --maintenance-token or --username/--password (or the QM_ANDROID_SMOKE_* env vars)"
        )

    adb("reverse", "tcp:8080", "tcp:8080")
    launch(clear_app=not args.preserve_app_data)
    sign_in(args.username, args.password, args.device_server_url)
    tap_tag("smoke-tab-reminders")
    wait_for_tag("smoke-reminder-screen")
    if fixture is not None:
        first_reminder_id = fixture["reminders"][0]["reminder_id"]
        ack_tag = f"smoke-reminder-ack-{first_reminder_id}"
        wait_for_tag(ack_tag, timeout=15.0)
        tap_tag(ack_tag)
        assert_tag_missing(ack_tag, timeout=15.0)
    else:
        wait_for_condition(
            "a reminder acknowledge action",
            lambda: bool(find_clickables_with_tag_prefix("smoke-reminder-ack-")),
            timeout=15.0,
        )
        first_ack = find_clickables_with_tag_prefix("smoke-reminder-ack-")[0]
        ack_tag = first_ack.resource_id.split("/")[-1]
        tap(first_ack)
        assert_tag_missing(ack_tag, timeout=15.0)
    if fixture is not None:
        open_reminder_payload(fixture["reminders"][1])
    else:
        tap_first_tag_prefix("smoke-reminder-open-")
    wait_for_tag("smoke-reminder-opened-banner")
    if fixture is not None:
        wait_for_tag(f"smoke-reminder-target-{fixture['reminders'][1]['batch_id']}")
    else:
        assert_text("Reminder target")
    tap_tag("smoke-reminder-opened-dismiss")
    if fixture is not None:
        assert_tag_missing(f"smoke-reminder-target-{fixture['reminders'][1]['batch_id']}")
    else:
        assert_text_missing("Reminder target")
    if fixture is not None:
        lifecycle_batch_id = fixture["reminders"][1]["batch_id"]
        tap_tag(f"smoke-inventory-batch-{lifecycle_batch_id}")
        wait_for_tag(f"smoke-selected-batch-{lifecycle_batch_id}")
        tap_tag(f"smoke-batch-discard-{lifecycle_batch_id}")
        wait_for_tag(f"smoke-batch-restore-{lifecycle_batch_id}", timeout=15.0)
        tap_tag(f"smoke-batch-restore-{lifecycle_batch_id}")
        assert_tag_missing(f"smoke-batch-restore-{lifecycle_batch_id}", timeout=15.0)
        wait_for_tag(f"smoke-batch-consume-{lifecycle_batch_id}", timeout=15.0)
    exercise_products(fixture)
    tap_tag("smoke-tab-settings")
    invite_code = None
    if fixture is None:
        tap_tag("smoke-create-invite-button")
        invite_code = wait_for_matching_text(INVITE_CODE_RE, timeout=15.0)
    else:
        invite_code = fixture["invite_code"]
    print(f"captured invite code {invite_code}")
    open_invite_link(invite_code, args.device_server_url)
    wait_for_tag("smoke-invite-handoff-card")
    assert_text(invite_code)
    wait_for_tag("smoke-sign-out-button")
    tap_tag("smoke-sign-out-button")
    sign_in(args.username, args.password)
    tap_tag("smoke-tab-settings")
    wait_for_tag("smoke-switch-household-header")
    print("Android UI smoke passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
