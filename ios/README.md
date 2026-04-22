# Quartermaster iOS

SwiftUI app, iOS 26 target, Liquid Glass design language. Built concurrently with the backend.

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
- Release configuration with `QUARTERMASTER_ASSOCIATED_DOMAIN` overridden to a non-placeholder hostname so the release validation script exercises the real contract

If GitHub-hosted macOS images drift and that simulator runtime disappears, update the workflow and this note together.

## Running against a local backend

1. Start the backend in another terminal:

   ```sh
   cargo run -p qm-server
   ```

2. Build + run the **Quartermaster** scheme in Xcode on an iOS 26 simulator. The simulator reaches the Mac host at `http://localhost:8080`, which is what the app uses by default.

3. First launch: the Onboarding screen appears with **Get started** selected. Enter a username + password, tap *Create household*, and the app should land on the Inventory tab with the three seeded locations (Pantry / Fridge / Freezer) showing empty states.

If the server rejects registration (`registration_disabled`), the backend has already had a first user created — either delete `data.db` at the repo root and restart the backend, or switch Onboarding to **Sign in**.

## Universal-link setup

Quartermaster keeps the custom `quartermaster://` scheme as a fallback, but iOS can also open invite links directly from HTTPS when the build has a matching Associated Domains entitlement.

1. Set `QM_PUBLIC_BASE_URL` on the server to the public HTTPS origin users will tap.
2. Serve the backend route at `/.well-known/apple-app-site-association` from that same origin.
3. Change `QUARTERMASTER_ASSOCIATED_DOMAIN` in `project.yml` from the placeholder `quartermaster.example.com` to that public host, then run `xcodegen generate`.
4. Build/install the app again so the entitlement matches the deployed host.

Release builds fail if `QUARTERMASTER_ASSOCIATED_DOMAIN` is still the placeholder host or is not a bare hostname. That guard is intentionally skipped for Debug builds so local development can keep using the custom scheme without a public HTTPS domain.

Do not point the associated-domain entitlement at `localhost`; keep local development on the custom scheme and use a real HTTPS host for universal links.

## Source layout

```
Quartermaster/
├── App/               Entry point, root phase switch, app-wide @Observable state
├── Core/
│   ├── Auth/          Keychain-backed TokenStore
│   └── Networking/    APIClient (URLSession + bearer + refresh rotation), DTOs, errors
├── Features/
│   ├── Onboarding/    Register / sign-in
│   ├── Main/          TabView shell
│   ├── Inventory/     List view that pulls /locations
│   ├── Scan/          Placeholder — VisionKit integration comes in slice 2
│   └── Settings/      Account info + sign out
└── DesignSystem/      Shared visual primitives (grows with Liquid Glass work)
```

## Networking notes

- DTOs in `Core/Networking/APIDTOs.swift` are hand-written to match the backend's OpenAPI schemas. When `apple/swift-openapi-generator` is wired up as an SPM build plugin, these will be replaced with generated types sourced from the checked-in `openapi.json` at the repo root.
- `APIClient` is an actor. A 401 on an authenticated request triggers one serialized refresh attempt, then retries.
- The default base URL is `http://localhost:8080` in the simulator; override it on the Onboarding screen when connecting to a self-hosted instance.
