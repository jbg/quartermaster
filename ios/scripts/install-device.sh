#!/bin/sh
set -eu

usage() {
	cat <<'EOF'
Usage: ios/scripts/install-device.sh [options]

Build, install, and optionally launch Quartermaster on a connected iPhone.

Options:
  --device DEVICE_ID                devicectl device identifier. Default: auto-detect
                                    when exactly one physical iOS device is connected.
  --configuration Debug|Release     Build configuration. Default: Debug.
  --derived-data-path PATH          DerivedData directory. Default: /tmp/qm-ios-build.
  --team TEAM_ID                    Development team id for signing.
  --bundle-id BUNDLE_ID             App bundle id for signing and launch.
  --associated-domain HOSTNAME      Associated domain override for Release builds.
  --no-launch                       Install without launching.
  -h, --help                        Show this help.

Environment equivalents:
  QM_IOS_DEVICE_ID
  QUARTERMASTER_IOS_DEVELOPMENT_TEAM
  QUARTERMASTER_IOS_BUNDLE_ID
  QUARTERMASTER_ASSOCIATED_DOMAIN
EOF
}

script_dir="$(CDPATH= cd -- "$(dirname "$0")" && pwd)"

device_id="${QM_IOS_DEVICE_ID:-}"
configuration="${QM_IOS_CONFIGURATION:-Debug}"
derived_data_path="${QM_IOS_DERIVED_DATA_PATH:-/tmp/qm-ios-build}"
team="${QUARTERMASTER_IOS_DEVELOPMENT_TEAM:-}"
bundle_id="${QUARTERMASTER_IOS_BUNDLE_ID:-}"
associated_domain="${QUARTERMASTER_ASSOCIATED_DOMAIN:-}"
launch=1

while [ "$#" -gt 0 ]; do
	case "$1" in
	--device)
		device_id="${2:?--device requires a value}"
		shift 2
		;;
	--configuration)
		configuration="${2:?--configuration requires a value}"
		shift 2
		;;
	--derived-data-path)
		derived_data_path="${2:?--derived-data-path requires a value}"
		shift 2
		;;
	--team)
		team="${2:?--team requires a value}"
		shift 2
		;;
	--bundle-id)
		bundle_id="${2:?--bundle-id requires a value}"
		shift 2
		;;
	--associated-domain)
		associated_domain="${2:?--associated-domain requires a value}"
		shift 2
		;;
	--no-launch)
		launch=0
		shift
		;;
	-h | --help)
		usage
		exit 0
		;;
	*)
		echo "error: unknown option: $1" >&2
		usage >&2
		exit 2
		;;
	esac
done

if [ -z "$team" ]; then
	echo "error: set QUARTERMASTER_IOS_DEVELOPMENT_TEAM or pass --team" >&2
	exit 2
fi

if [ -z "$bundle_id" ]; then
	echo "error: set QUARTERMASTER_IOS_BUNDLE_ID or pass --bundle-id" >&2
	exit 2
fi

if [ -z "$device_id" ]; then
	device_id="$(
		xcrun xctrace list devices 2>/dev/null |
			awk '
				/\([0-9A-Fa-f-]{25,}\)/ && $0 !~ /Simulator/ && $0 !~ /^==/ {
					line = $0
					sub(/^ */, "", line)
					match(line, /\(([0-9A-Fa-f-]{25,})\)/)
					if (RSTART > 0) {
						print substr(line, RSTART + 1, RLENGTH - 2)
					}
				}
			' |
			head -n 2
	)"
	device_count="$(printf '%s\n' "$device_id" | sed '/^$/d' | wc -l | tr -d ' ')"
	if [ "$device_count" -ne 1 ]; then
		echo "error: could not auto-detect exactly one physical iOS device; pass --device" >&2
		xcrun xctrace list devices >&2 || true
		exit 2
	fi
fi

set -- \
	--configuration "$configuration" \
	--destination "generic/platform=iOS" \
	--derived-data-path "$derived_data_path" \
	--team "$team" \
	--bundle-id "$bundle_id"

if [ -n "$associated_domain" ]; then
	set -- "$@" --associated-domain "$associated_domain"
fi

sh "$script_dir/build-app.sh" "$@"

app_path="$derived_data_path/Build/Products/$configuration-iphoneos/Quartermaster.app"

xcrun devicectl device install app --device "$device_id" "$app_path"

if [ "$launch" -eq 1 ]; then
	xcrun devicectl device process launch --device "$device_id" "$bundle_id"
fi
