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

## Running against a local backend

1. Start the backend in another terminal:

   ```sh
   cargo run -p qm-server
   ```

2. Build + run the **Quartermaster** scheme in Xcode on an iOS 26 simulator. The simulator reaches the Mac host at `http://localhost:8080`, which is what the app uses by default.

3. First launch: the Onboarding screen appears with **Get started** selected. Enter a username + password, tap *Create household*, and the app should land on the Inventory tab with the three seeded locations (Pantry / Fridge / Freezer) showing empty states.

If the server rejects registration (`registration_disabled`), the backend has already had a first user created — either delete `data.db` at the repo root and restart the backend, or switch Onboarding to **Sign in**.

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
