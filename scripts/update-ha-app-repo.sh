#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
	echo "usage: $0 <home-assistant-apps-repo-path>" >&2
	exit 2
fi

repo_path="$1"

: "${RELEASE_VERSION:?RELEASE_VERSION is required}"
: "${RELEASE_TAG:?RELEASE_TAG is required}"
if [ -z "${RELEASE_NOTES:-}" ]; then
	RELEASE_NOTES="No release notes provided."
fi
export RELEASE_NOTES

if [ ! -f "$repo_path/quartermaster/config.yaml" ]; then
	echo "missing $repo_path/quartermaster/config.yaml" >&2
	exit 1
fi

python3 - "$repo_path" <<'PY'
import os
import pathlib
import sys

repo = pathlib.Path(sys.argv[1])
version = os.environ["RELEASE_VERSION"]
tag = os.environ["RELEASE_TAG"]
notes = os.environ["RELEASE_NOTES"].strip() or "No release notes provided."

config_path = repo / "quartermaster" / "config.yaml"
lines = config_path.read_text().splitlines()
for idx, line in enumerate(lines):
    if line.startswith("version:"):
        lines[idx] = f'version: "{version}"'
        break
else:
    raise SystemExit("quartermaster/config.yaml is missing a top-level version field")
config_path.write_text("\n".join(lines) + "\n")

changelog_path = repo / "quartermaster" / "CHANGELOG.md"
existing = changelog_path.read_text() if changelog_path.exists() else "# Changelog\n"
header = f"## {tag}\n\n{notes}\n\n"
if existing.startswith("# Changelog\n"):
    existing = existing.replace("# Changelog\n", "# Changelog\n\n" + header, 1)
else:
    existing = "# Changelog\n\n" + header + existing
changelog_path.write_text(existing)
PY

python3 - "$repo_path" <<'PY'
import os
import pathlib
import sys

repo = pathlib.Path(sys.argv[1])
config = (repo / "quartermaster" / "config.yaml").read_text()

assert "image: ghcr.io/jbg/quartermaster" in config
assert "\nlegacy:" not in config
assert f'version: "{os.environ["RELEASE_VERSION"]}"' in config
assert "  - amd64" in config
assert "  - aarch64" in config
PY
