# Quartermaster iOS

SwiftUI app, iOS 26 target, Liquid Glass design language.

## Generating the Xcode project

The Xcode project is generated from `project.yml` by [XcodeGen](https://github.com/yonaskolb/XcodeGen). `.xcodeproj` is not checked in.

```sh
brew install xcodegen    # one-time
cd ios
xcodegen generate
open Quartermaster.xcodeproj
```

Re-run `xcodegen generate` whenever `project.yml` or the source layout changes.

The checked-in CI build uses:

- simulator destination `platform=iOS Simulator,name=iPhone 17 Pro,OS=26.2`
- `-skipPackagePluginValidation` because `swift-openapi-generator` runs as an Xcode build-tool plugin during the build
- Release configuration generated from CI environment variables rather than checked-in Apple identity

If GitHub-hosted macOS images drift and that simulator runtime disappears, update the workflow and this note together.

Simulator-backed `xcodebuild` runs are host-only checks. Run them from a normal macOS shell, not from inside the Codex sandbox.

## Running against a local backend

1. Start the backend in another terminal:

   ```sh
   cargo run -p qm-server
   ```

2. Build + run the **Quartermaster** scheme in Xcode on an iOS 26 simulator. The simulator reaches the Mac host at `http://localhost:8080`, which is what the app uses by default.

3. First launch: the Onboarding screen appears with **Get started** selected. Enter a username + password, tap _Create household_, and the app should land on the Inventory tab with the three seeded locations (Pantry / Fridge / Freezer) showing empty states.

If the server rejects registration (`registration_disabled`), the backend has already had a first user created — either delete `data.db` at the repo root and restart the backend, or switch Onboarding to **Sign in**.

## Installing on an iPhone from Terminal

You can build, install, and launch the app on a connected iPhone without opening Xcode:

```sh
QUARTERMASTER_IOS_DEVELOPMENT_TEAM=YOUR_TEAM_ID \
QUARTERMASTER_IOS_BUNDLE_ID=com.yourname.Quartermaster \
  sh ios/scripts/install-device.sh
```

The script auto-detects the device when exactly one physical iOS device is connected. If you have multiple devices attached, pass `--device DEVICE_ID`; you can list devices with `xcrun xctrace list devices`.

If Xcode has already installed the app but the script reports missing profiles, the bundle id may not match the one Xcode provisioned. List local profiles to inspect the bundle ids Xcode has already used:

```sh
sh ios/scripts/list-signing-profiles.sh
```

For Xcode-managed profiles named `iOS Team Provisioning Profile: ...`, reuse the bundle id but omit `QUARTERMASTER_IOS_PROFILE` so Xcode can manage signing and capabilities:

```sh
QUARTERMASTER_IOS_DEVELOPMENT_TEAM=YOUR_TEAM_ID \
QUARTERMASTER_IOS_BUNDLE_ID=com.yourname.Quartermaster \
  sh ios/scripts/install-device.sh
```

Only set `QUARTERMASTER_IOS_PROFILE` for a manually managed provisioning profile.

For the latest local checkout, pull first, then run the installer:

```sh
git pull --ff-only
QUARTERMASTER_IOS_DEVELOPMENT_TEAM=YOUR_TEAM_ID \
QUARTERMASTER_IOS_BUNDLE_ID=com.yourname.Quartermaster \
  sh ios/scripts/install-device.sh
```

The shared build entry point is `ios/scripts/build-app.sh`. CI uses the same script in simulator build-only mode, while `install-device.sh` wraps it with physical-device install and launch steps.

## TestFlight release setup

Merging a Release Please PR creates a GitHub release and calls the release workflow. That workflow archives the iOS app, exports an App Store Connect IPA, and uploads it to TestFlight with fastlane `pilot`.

Apple account prerequisites:

- A paid Apple Developer Program team.
- An explicit app ID for the release bundle id, with Push Notifications and Associated Domains enabled.
- An App Store Connect app record for the same bundle id.
- An Apple Distribution certificate exported as a password-protected `.p12`.
- An App Store provisioning profile for the same bundle id.
- An App Store Connect API key with access to TestFlight uploads and provisioning profile downloads.

GitHub repository variables:

```text
APPSTORE_API_KEY_ID
APPSTORE_ISSUER_ID
QUARTERMASTER_IOS_DEVELOPMENT_TEAM
QUARTERMASTER_IOS_BUNDLE_ID
QUARTERMASTER_ASSOCIATED_DOMAIN
```

GitHub repository secrets:

```text
APPSTORE_API_PRIVATE_KEY
APPSTORE_CERTIFICATES_FILE_BASE64
APPSTORE_CERTIFICATES_PASSWORD
```

Create `APPSTORE_CERTIFICATES_FILE_BASE64` from the exported `.p12`:

```sh
base64 -i ios_distribution.p12 | pbcopy
```

The archive/export entry point is reusable locally:

```sh
QUARTERMASTER_IOS_DEVELOPMENT_TEAM=YOUR_TEAM_ID \
QUARTERMASTER_IOS_BUNDLE_ID=com.yourname.Quartermaster \
QUARTERMASTER_ASSOCIATED_DOMAIN=quartermaster.example.com \
  sh ios/scripts/archive-app.sh --version 1.2.3 --build-number 123 --print-ipa-path
```

`archive-app.sh` defaults to `Apple Distribution` for release archive signing and export. Set optional `QUARTERMASTER_IOS_SIGNING_CERTIFICATE` or pass `--signing-certificate` only if the imported distribution certificate needs a different identity string or SHA.

### fastlane metadata and TestFlight automation

App Store listing metadata lives in `ios/fastlane/metadata/`, and screenshots should be added under `ios/fastlane/screenshots/`. Keep listing text reviewable there instead of clicking around in App Store Connect.

Install the pinned fastlane bundle with Ruby 3.0 or newer:

```sh
cd ios
bundle install
```

fastlane uses the same App Store Connect API-key environment variables as CI:

```text
APPSTORE_API_KEY_ID
APPSTORE_ISSUER_ID
APPSTORE_API_PRIVATE_KEY
QUARTERMASTER_IOS_BUNDLE_ID
```

Useful local commands:

```sh
cd ios
bundle exec fastlane ios check_metadata
bundle exec fastlane ios upload_metadata
QM_IOS_IPA_PATH=/path/to/Quartermaster.ipa bundle exec fastlane ios upload_testflight
```

`upload_metadata` sends listing metadata only and does not submit the app for review. `upload_testflight` is also what CI calls after `archive-app.sh` exports the IPA.

## Universal-link setup

Quartermaster keeps the custom `quartermaster://` scheme as a fallback, but iOS can also open invite links directly from HTTPS when the build has a matching Associated Domains entitlement.

1. Choose the public HTTPS associated-domain host users will tap, such as `quartermaster.example.com`.
2. Set `QM_IOS_TEAM_ID` and `QM_IOS_BUNDLE_ID` on the server so the backend can serve `/.well-known/apple-app-site-association` for the matching app build.
3. Export `QUARTERMASTER_IOS_DEVELOPMENT_TEAM`, `QUARTERMASTER_IOS_BUNDLE_ID`, and `QUARTERMASTER_ASSOCIATED_DOMAIN`, then run:

   ```sh
   /bin/sh scripts/generate-release-config.sh
   xcodegen generate
   ```

4. Build/install the Release app again so the entitlement matches the deployed host.

`QM_PUBLIC_BASE_URL` may still be set to an HTTP origin for LAN app setup codes and browser invite fallbacks. iOS Universal Links only use HTTPS associated domains.

Release builds fail if `DEVELOPMENT_TEAM`, `PRODUCT_BUNDLE_IDENTIFIER`, or `QUARTERMASTER_ASSOCIATED_DOMAIN` are missing or malformed. That guard is intentionally skipped for Debug builds so local development can keep using the custom scheme without a public HTTPS domain.

If the server does not have `QM_IOS_TEAM_ID` and `QM_IOS_BUNDLE_ID` configured, the backend returns `404` for `/.well-known/apple-app-site-association` and the app falls back to the custom `quartermaster://` scheme plus manual invite entry.

Do not point the associated-domain entitlement at `localhost`; keep local development on the custom scheme and use a real HTTPS host for universal links.

## Source layout

```
Quartermaster/
├── App/               Entry point, root phase switch, app-wide @Observable state
├── Core/
│   ├── Auth/          Keychain-backed TokenStore
│   └── Networking/    APIClient (URLSession + bearer + refresh rotation), DTOs, errors
├── Features/
│   ├── AddStock/      Product search, manual product entry, and stock creation
│   ├── History/       Stock event history and batch detail recovery actions
│   ├── Households/    Shared household switching, redeem, and create surfaces
│   ├── Inventory/     Grouped inventory, filters, batch edit/consume/discard/restore
│   ├── Main/          TabView shell
│   ├── Onboarding/    Register / sign-in
│   ├── Products/      Product detail, edit, delete, and restore
│   ├── Reminders/     Durable reminder inbox
│   ├── Scan/          VisionKit barcode scanning on supported physical devices
│   └── Settings/      Household settings, locations, members, invites, and sign out
└── DesignSystem/      Shared visual primitives (grows with Liquid Glass work)
```

## Networking notes

- DTOs and the generated `Client` come from the checked-in `openapi.json` via the `swift-openapi-generator` build-tool plugin. Keep hand-written extensions in `Core/Networking/APIAliases.swift`.
- `APIClient` is an actor façade over the generated client. A 401 on an authenticated request triggers one serialized refresh attempt, then retries.
- `GET /api/v1/auth/me` exposes `current_household` as a nullable object; use that shared shape instead of flattening active-household fields in client code.
- The default base URL is `http://localhost:8080` in the simulator; override it on the Onboarding screen when connecting to a self-hosted instance.

## Verification

Host-only iOS reminder-state verification:

```sh
xcodebuild -project ios/Quartermaster.xcodeproj \
  -scheme Quartermaster \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro,OS=26.2' \
  -skipPackagePluginValidation \
  test
```

This should execute `QuartermasterTests/AppStateReminderTests.swift`, not just build the test bundle.
