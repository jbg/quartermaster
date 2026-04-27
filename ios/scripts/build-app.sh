#!/bin/sh
set -eu

usage() {
	cat <<'EOF'
Usage: ios/scripts/build-app.sh [options]

Build the Quartermaster iOS app through the same xcodebuild entry point used by CI.

Options:
  --configuration Debug|Release     Build configuration. Default: Debug.
  --destination DESTINATION         xcodebuild destination.
                                    Default: generic/platform=iOS for Debug,
                                    or CI simulator for Release.
  --derived-data-path PATH          DerivedData directory. Default: /tmp/qm-ios-build.
  --associated-domain HOSTNAME      Override QUARTERMASTER_ASSOCIATED_DOMAIN.
  --team TEAM_ID                    Override DEVELOPMENT_TEAM.
  --bundle-id BUNDLE_ID             Override PRODUCT_BUNDLE_IDENTIFIER.
  --profile PROFILE_NAME            Use a local provisioning profile by name.
                                    Implies manual signing.
  --action ACTION                   xcodebuild action. Default: build.
  --print-app-path                  Print the resulting .app path after a successful build.
  -h, --help                        Show this help.

Environment overrides:
  QUARTERMASTER_IOS_DEVELOPMENT_TEAM
  QUARTERMASTER_IOS_BUNDLE_ID
  QUARTERMASTER_IOS_PROFILE
  QUARTERMASTER_ASSOCIATED_DOMAIN
  QM_IOS_CONFIGURATION
  QM_IOS_DESTINATION
  QM_IOS_DERIVED_DATA_PATH
EOF
}

script_dir="$(CDPATH= cd -- "$(dirname "$0")" && pwd)"
ios_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"

configuration="${QM_IOS_CONFIGURATION:-Debug}"
destination="${QM_IOS_DESTINATION:-}"
derived_data_path="${QM_IOS_DERIVED_DATA_PATH:-/tmp/qm-ios-build}"
action="build"
print_app_path=0
team="${QUARTERMASTER_IOS_DEVELOPMENT_TEAM:-}"
bundle_id="${QUARTERMASTER_IOS_BUNDLE_ID:-}"
profile="${QUARTERMASTER_IOS_PROFILE:-}"
associated_domain="${QUARTERMASTER_ASSOCIATED_DOMAIN:-}"

while [ "$#" -gt 0 ]; do
	case "$1" in
	--configuration)
		configuration="${2:?--configuration requires a value}"
		shift 2
		;;
	--destination)
		destination="${2:?--destination requires a value}"
		shift 2
		;;
	--derived-data-path)
		derived_data_path="${2:?--derived-data-path requires a value}"
		shift 2
		;;
	--associated-domain)
		associated_domain="${2:?--associated-domain requires a value}"
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
	--profile)
		profile="${2:?--profile requires a value}"
		shift 2
		;;
	--action)
		action="${2:?--action requires a value}"
		shift 2
		;;
	--print-app-path)
		print_app_path=1
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

case "$configuration" in
Debug | Release) ;;
*)
	echo "error: --configuration must be Debug or Release" >&2
	exit 2
	;;
esac

if [ -z "$destination" ]; then
	if [ "$configuration" = "Release" ]; then
		destination="platform=iOS Simulator,name=iPhone 17 Pro,OS=26.2"
	else
		destination="generic/platform=iOS"
	fi
fi

if [ "$configuration" = "Release" ]; then
	if [ -n "$team" ] && [ -n "$bundle_id" ] && [ -n "$associated_domain" ]; then
		QUARTERMASTER_IOS_DEVELOPMENT_TEAM="$team" \
			QUARTERMASTER_IOS_BUNDLE_ID="$bundle_id" \
			QUARTERMASTER_ASSOCIATED_DOMAIN="$associated_domain" \
			sh "$script_dir/generate-release-config.sh"
	fi
fi

set -- \
	-project "$ios_dir/Quartermaster.xcodeproj" \
	-scheme Quartermaster \
	-configuration "$configuration" \
	-destination "$destination" \
	-derivedDataPath "$derived_data_path" \
	-skipPackagePluginValidation

if [ -n "$team" ]; then
	set -- "$@" DEVELOPMENT_TEAM="$team"
fi

if [ -n "$bundle_id" ]; then
	set -- "$@" PRODUCT_BUNDLE_IDENTIFIER="$bundle_id"
fi

if [ -n "$profile" ]; then
	set -- "$@" CODE_SIGN_STYLE=Manual PROVISIONING_PROFILE_SPECIFIER="$profile"
fi

if [ -n "$associated_domain" ]; then
	set -- "$@" QUARTERMASTER_ASSOCIATED_DOMAIN="$associated_domain"
fi

if { [ "$destination" = "generic/platform=iOS" ] || [ "$destination" = "platform=iOS" ]; } && [ -z "$profile" ]; then
	set -- "$@" -allowProvisioningUpdates
fi

xcodebuild "$@" "$action"

if [ "$print_app_path" -eq 1 ]; then
	if [ "$destination" = "generic/platform=iOS" ] || [ "$destination" = "platform=iOS" ]; then
		platform_dir="iphoneos"
	else
		platform_dir="iphonesimulator"
	fi
	printf '%s\n' "$derived_data_path/Build/Products/$configuration-$platform_dir/Quartermaster.app"
fi
