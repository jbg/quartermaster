#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

scheme="${QM_IOS_SCREENSHOT_SCHEME:-QuartermasterAppStoreScreenshots}"
destination="${QM_IOS_SCREENSHOT_DESTINATION:-platform=iOS Simulator,name=iPhone 17 Pro,OS=26.2}"
result_bundle="${QM_IOS_SCREENSHOT_RESULT_BUNDLE:-fastlane/screenshot-results.xcresult}"
output_dir="${QM_IOS_SCREENSHOT_OUTPUT_DIR:-fastlane/screenshots-local/en-US}"

rm -rf "$result_bundle" "$output_dir"
mkdir -p "$output_dir"

xcodebuild \
	-project Quartermaster.xcodeproj \
	-scheme "$scheme" \
	-destination "$destination" \
	-skipPackagePluginValidation \
	-resultBundlePath "$result_bundle" \
	-skip-testing:QuartermasterTests \
	-only-testing:QuartermasterUITests/AppStoreScreenshotUITests/testCaptureCoreInventoryStory \
	test

raw_output="$output_dir/raw"
mkdir -p "$raw_output"

xcrun xcresulttool export attachments \
	--path "$result_bundle" \
	--output-path "$raw_output"

ruby -rjson -e '
  output_dir = ARGV.fetch(0)
  raw_output = ARGV.fetch(1)
  manifest = JSON.parse(File.read(File.join(raw_output, "manifest.json")))
  attachments = manifest.flat_map { |test| test.fetch("attachments", []) }
  attachments
    .select { |attachment| attachment.fetch("exportedFileName").end_with?(".png") }
    .sort_by { |attachment| attachment.fetch("suggestedHumanReadableName") }
    .each do |attachment|
      source = File.join(raw_output, attachment.fetch("exportedFileName"))
      name = attachment.fetch("suggestedHumanReadableName").sub(/_0_[0-9A-F-]+\.png\z/, ".png")
      File.binwrite(File.join(output_dir, name), File.binread(source))
    end
' "$output_dir" "$raw_output"

rm -rf "$raw_output"
count="$(find "$output_dir" -type f -name '*.png' | wc -l | tr -d ' ')"

if [ "$count" -eq 0 ]; then
	echo "error: no screenshots were exported from $result_bundle" >&2
	exit 1
fi

echo "Wrote $count screenshots to ios/$output_dir"
