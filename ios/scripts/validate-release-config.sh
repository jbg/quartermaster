#!/bin/sh
set -eu

if [ "${CONFIGURATION:-}" != "Release" ]; then
	exit 0
fi

team="${DEVELOPMENT_TEAM:-}"
bundle="${PRODUCT_BUNDLE_IDENTIFIER:-}"
domain="${QUARTERMASTER_ASSOCIATED_DOMAIN:-}"

if [ -z "$team" ]; then
	echo "error: DEVELOPMENT_TEAM must be resolved for Release builds" >&2
	exit 1
fi

if [ -z "$bundle" ]; then
	echo "error: PRODUCT_BUNDLE_IDENTIFIER must be resolved for Release builds" >&2
	exit 1
fi

if [ -z "$domain" ]; then
	echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be set for Release builds" >&2
	exit 1
fi

if ! printf '%s' "$team" | grep -Eq '^[A-Za-z0-9]+$'; then
	echo "error: DEVELOPMENT_TEAM must be ASCII alphanumeric" >&2
	exit 1
fi

if ! printf '%s' "$bundle" | grep -Eq '^[A-Za-z0-9.-]+$'; then
	echo "error: PRODUCT_BUNDLE_IDENTIFIER must contain only ASCII alphanumeric characters, dots, or hyphens" >&2
	exit 1
fi

case "$domain" in
*://* | */* | *\?* | *#* | *:* | *' '*)
	echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be a bare hostname" >&2
	exit 1
	;;
esac

if ! printf '%s' "$domain" | grep -Eq '^[A-Za-z0-9.-]+$'; then
	echo "error: QUARTERMASTER_ASSOCIATED_DOMAIN must be a bare hostname" >&2
	exit 1
fi
