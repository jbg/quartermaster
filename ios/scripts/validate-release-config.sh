#!/bin/sh
set -eu

if [ "${CONFIGURATION:-}" != "Release" ]; then
  exit 0
fi

domain="${QUARTERMASTER_ASSOCIATED_DOMAIN:-}"

if [ -z "$domain" ]; then
  echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be set for Release builds" >&2
  exit 1
fi

if [ "$domain" = "quartermaster.example.com" ]; then
  echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN still uses the placeholder host in Release" >&2
  exit 1
fi

case "$domain" in
  *://*|*/*|*\?*|*#*|*:*|*' '*)
    echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be a bare hostname" >&2
    exit 1
    ;;
esac

if ! printf '%s' "$domain" | grep -Eq '^[A-Za-z0-9.-]+$'; then
  echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be a bare hostname" >&2
  exit 1
fi
