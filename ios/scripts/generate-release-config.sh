#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
out="$root_dir/Config/ReleaseIdentity.generated.xcconfig"

team="${QUARTERMASTER_IOS_DEVELOPMENT_TEAM:-}"
bundle="${QUARTERMASTER_IOS_BUNDLE_ID:-}"
domain="${QUARTERMASTER_ASSOCIATED_DOMAIN:-}"

if [ -z "$team" ]; then
  echo "error: QUARTERMASTER_IOS_DEVELOPMENT_TEAM must be set" >&2
  exit 1
fi

if [ -z "$bundle" ]; then
  echo "error: QUARTERMASTER_IOS_BUNDLE_ID must be set" >&2
  exit 1
fi

if [ -z "$domain" ]; then
  echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be set" >&2
  exit 1
fi

case "$domain" in
  *://*|*/*|*\?*|*#*|*:*|*' '*)
    echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be a bare hostname" >&2
    exit 1
    ;;
esac

if ! printf '%s' "$team" | grep -Eq '^[A-Za-z0-9]+$'; then
  echo "error: QUARTERMASTER_IOS_DEVELOPMENT_TEAM must be ASCII alphanumeric" >&2
  exit 1
fi

if ! printf '%s' "$bundle" | grep -Eq '^[A-Za-z0-9.-]+$'; then
  echo "error: QUARTERMASTER_IOS_BUNDLE_ID must contain only ASCII alphanumeric characters, dots, or hyphens" >&2
  exit 1
fi

if ! printf '%s' "$domain" | grep -Eq '^[A-Za-z0-9.-]+$'; then
  echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be a bare hostname" >&2
  exit 1
fi

mkdir -p "$(dirname "$out")"
cat >"$out" <<EOF
DEVELOPMENT_TEAM = $team
PRODUCT_BUNDLE_IDENTIFIER = $bundle
QUARTERMASTER_ASSOCIATED_DOMAIN = $domain
EOF

echo "wrote $out"
