#!/bin/sh
set -eu

if [ "$#" -gt 0 ]; then
	set -- "$@"
else
	set -- \
		"$HOME/Library/MobileDevice/Provisioning Profiles" \
		"$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
fi

find "$@" -name '*.mobileprovision' -print 2>/dev/null |
	while IFS= read -r profile; do
		plist="$(mktemp)"
		if ! security cms -D -i "$profile" >"$plist" 2>/dev/null; then
			rm -f "$plist"
			continue
		fi

		name="$(plutil -extract Name raw -o - "$plist" 2>/dev/null || true)"
		uuid="$(plutil -extract UUID raw -o - "$plist" 2>/dev/null || true)"
		team="$(plutil -extract TeamIdentifier.0 raw -o - "$plist" 2>/dev/null || true)"
		app_id="$(plutil -extract Entitlements.application-identifier raw -o - "$plist" 2>/dev/null || true)"
		expires="$(plutil -extract ExpirationDate raw -o - "$plist" 2>/dev/null || true)"

		rm -f "$plist"

		case "$app_id" in
		"$team".*) bundle_id="${app_id#"$team".}" ;;
		*) bundle_id="$app_id" ;;
		esac

		if [ -n "$name" ] && [ -n "$team" ] && [ -n "$bundle_id" ]; then
			printf 'name: %s\nteam: %s\nbundle_id: %s\nuuid: %s\nexpires: %s\n\n' \
				"$name" "$team" "$bundle_id" "$uuid" "$expires"
		fi
	done
