#!/bin/sh
set -eu

usage() {
	cat <<'EOF'
Usage: ios/scripts/archive-app.sh [options]

Archive and export Quartermaster as an App Store Connect IPA.

Options:
  --archive-path PATH               .xcarchive output path.
                                    Default: /tmp/qm-ios-release/Quartermaster.xcarchive.
  --export-path PATH                IPA export directory.
                                    Default: /tmp/qm-ios-release/export.
  --derived-data-path PATH          DerivedData directory.
                                    Default: /tmp/qm-ios-release/DerivedData.
  --version VERSION                 MARKETING_VERSION override.
  --build-number NUMBER             CURRENT_PROJECT_VERSION override.
  --team TEAM_ID                    Development team id for signing.
  --bundle-id BUNDLE_ID             App bundle id for signing.
  --profile PROFILE_NAME            App Store provisioning profile name.
                                    Default: auto-detect an installed profile
                                    for the bundle id.
  --signing-certificate NAME        Signing certificate identity for archive
                                    and export. Default: Apple Distribution.
  --associated-domain HOSTNAME      Associated domain for Release entitlements.
  --print-ipa-path                  Print the exported IPA path after success.
  --ipa-path-file PATH              Write the exported IPA path to PATH.
  -h, --help                        Show this help.

Environment equivalents:
  QUARTERMASTER_IOS_DEVELOPMENT_TEAM
  QUARTERMASTER_IOS_BUNDLE_ID
  QUARTERMASTER_IOS_PROFILE
  QUARTERMASTER_IOS_SIGNING_CERTIFICATE
  QUARTERMASTER_ASSOCIATED_DOMAIN
  QM_IOS_MARKETING_VERSION
  QM_IOS_BUILD_NUMBER
  QM_IOS_ARCHIVE_PATH
  QM_IOS_EXPORT_PATH
  QM_IOS_DERIVED_DATA_PATH
EOF
}

script_dir="$(CDPATH= cd -- "$(dirname "$0")" && pwd)"
ios_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"

archive_path="${QM_IOS_ARCHIVE_PATH:-/tmp/qm-ios-release/Quartermaster.xcarchive}"
export_path="${QM_IOS_EXPORT_PATH:-/tmp/qm-ios-release/export}"
derived_data_path="${QM_IOS_DERIVED_DATA_PATH:-/tmp/qm-ios-release/DerivedData}"
version="${QM_IOS_MARKETING_VERSION:-}"
build_number="${QM_IOS_BUILD_NUMBER:-}"
team="${QUARTERMASTER_IOS_DEVELOPMENT_TEAM:-}"
bundle_id="${QUARTERMASTER_IOS_BUNDLE_ID:-}"
profile="${QUARTERMASTER_IOS_PROFILE:-}"
signing_certificate="${QUARTERMASTER_IOS_SIGNING_CERTIFICATE:-Apple Distribution}"
associated_domain="${QUARTERMASTER_ASSOCIATED_DOMAIN:-}"
print_ipa_path=0
ipa_path_file="${QM_IOS_IPA_PATH_FILE:-}"

while [ "$#" -gt 0 ]; do
	case "$1" in
	--archive-path)
		archive_path="${2:?--archive-path requires a value}"
		shift 2
		;;
	--export-path)
		export_path="${2:?--export-path requires a value}"
		shift 2
		;;
	--derived-data-path)
		derived_data_path="${2:?--derived-data-path requires a value}"
		shift 2
		;;
	--version)
		version="${2:?--version requires a value}"
		shift 2
		;;
	--build-number)
		build_number="${2:?--build-number requires a value}"
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
	--signing-certificate)
		signing_certificate="${2:?--signing-certificate requires a value}"
		shift 2
		;;
	--associated-domain)
		associated_domain="${2:?--associated-domain requires a value}"
		shift 2
		;;
	--print-ipa-path)
		print_ipa_path=1
		shift
		;;
	--ipa-path-file)
		ipa_path_file="${2:?--ipa-path-file requires a value}"
		shift 2
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

if [ -z "$associated_domain" ]; then
	echo "error: set QUARTERMASTER_ASSOCIATED_DOMAIN or pass --associated-domain" >&2
	exit 2
fi

if [ -z "$profile" ]; then
	profile="$(
		sh "$script_dir/list-signing-profiles.sh" |
			awk -v bundle="$bundle_id" '
				$1 == "name:" {
					$1 = ""
					sub(/^ /, "")
					name = $0
				}
				$1 == "bundle_id:" {
					matches_bundle = ($2 == bundle)
				}
				$1 == "distribution:" && matches_bundle && $2 == "app_store" {
					print name
					exit
				}
				$0 == "" {
					name = ""
					matches_bundle = 0
				}
			'
	)"
fi

if [ -z "$profile" ]; then
	echo "error: no installed App Store provisioning profile found for $bundle_id; pass --profile" >&2
	exit 2
fi

if [ -z "$signing_certificate" ]; then
	echo "error: --signing-certificate must not be empty" >&2
	exit 2
fi

QUARTERMASTER_IOS_DEVELOPMENT_TEAM="$team" \
	QUARTERMASTER_IOS_BUNDLE_ID="$bundle_id" \
	QUARTERMASTER_ASSOCIATED_DOMAIN="$associated_domain" \
	sh "$script_dir/generate-release-config.sh"

rm -rf "$archive_path" "$export_path"
mkdir -p "$(dirname "$archive_path")" "$export_path"

set -- \
	-project "$ios_dir/Quartermaster.xcodeproj" \
	-scheme Quartermaster \
	-configuration Release \
	-destination "generic/platform=iOS" \
	-archivePath "$archive_path" \
	-derivedDataPath "$derived_data_path" \
	-skipPackagePluginValidation \
	DEVELOPMENT_TEAM="$team" \
	PRODUCT_BUNDLE_IDENTIFIER="$bundle_id" \
	QUARTERMASTER_ASSOCIATED_DOMAIN="$associated_domain" \
	"CODE_SIGN_STYLE[sdk=iphoneos*]=Manual" \
	"CODE_SIGN_IDENTITY[sdk=iphoneos*]=$signing_certificate" \
	"PROVISIONING_PROFILE_SPECIFIER[sdk=iphoneos*]=$profile"

if [ -n "$version" ]; then
	set -- "$@" MARKETING_VERSION="$version"
fi

if [ -n "$build_number" ]; then
	set -- "$@" CURRENT_PROJECT_VERSION="$build_number"
fi

xcodebuild "$@" archive

export_options="$(mktemp)"
cp "$ios_dir/ExportOptions.plist" "$export_options"
/usr/libexec/PlistBuddy -c "Set :teamID $team" "$export_options" 2>/dev/null ||
	/usr/libexec/PlistBuddy -c "Add :teamID string $team" "$export_options"
/usr/libexec/PlistBuddy -c "Add :provisioningProfiles dict" "$export_options" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Set :provisioningProfiles:$bundle_id $profile" "$export_options" 2>/dev/null ||
	/usr/libexec/PlistBuddy -c "Add :provisioningProfiles:$bundle_id string $profile" "$export_options"
/usr/libexec/PlistBuddy -c "Set :signingCertificate $signing_certificate" "$export_options" 2>/dev/null ||
	/usr/libexec/PlistBuddy -c "Add :signingCertificate string $signing_certificate" "$export_options"

xcodebuild \
	-exportArchive \
	-archivePath "$archive_path" \
	-exportPath "$export_path" \
	-exportOptionsPlist "$export_options"

rm -f "$export_options"

ipa_path="$export_path/Quartermaster.ipa"
if [ ! -f "$ipa_path" ]; then
	ipa_path="$(find "$export_path" -maxdepth 1 -name '*.ipa' -print -quit)"
fi

if [ -z "$ipa_path" ] || [ ! -f "$ipa_path" ]; then
	echo "error: no IPA was exported to $export_path" >&2
	exit 1
fi

if [ -n "$ipa_path_file" ]; then
	mkdir -p "$(dirname "$ipa_path_file")"
	printf '%s\n' "$ipa_path" >"$ipa_path_file"
fi

if [ "$print_ipa_path" -eq 1 ]; then
	printf '%s\n' "$ipa_path"
fi
