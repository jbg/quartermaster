# Contributing

Quartermaster is usable for self-hosted households and still evolving quickly. Please open an issue to discuss larger feature ideas before writing code.

Use small, focused pull requests and Conventional Commits:

- `fix:` for patch releases
- `feat:` for minor releases
- `type!:` or a `BREAKING CHANGE:` footer for major releases
- `chore:`, `docs:`, `test:`, or `refactor:` when the change should not bump the product version

## Repository Layout

```text
.
|-- Cargo.toml              workspace manifest
|-- crates/
|   |-- qm-core/            domain logic; no I/O
|   |-- qm-db/              SQLx repositories and migrations
|   |-- qm-api/             Axum handlers, middleware, OpenAPI, integration tests
|   |-- qm-suppliers/       supplier integration boundary and mock supplier
|   `-- qm-server/          shipped API and background worker binary
|-- xtask/                  developer tasks
|-- openapi.json            canonical generated API spec
|-- android/                Jetpack Compose app and generated Retrofit client
|-- web/                    SvelteKit web shell and generated TypeScript client
`-- ios/Quartermaster/      SwiftUI app and iOS OpenAPI copy
```

## OpenAPI

Regenerate the OpenAPI spec after any Rust DTO, route, or enum change:

```sh
cargo xtask export-openapi
```

This writes both:

- `openapi.json`
- `ios/Quartermaster/openapi.json`

Commit both copies. The iOS Xcode build-tool plugin reads the iOS copy, while Android and web generation use the repo-root copy.

Native client DTOs are generated from OpenAPI. Do not hand-edit generated DTOs on any client. iOS extensions, typealiases, compatibility helpers, and JSON Patch request aliases live in `ios/Quartermaster/Core/Networking/APIAliases.swift`.

## Rust Verification

```sh
cargo deny check advisories bans licenses sources
cargo test --workspace
cargo xtask verify-release-config
cargo xtask verify-stock-ledger
```

`cargo deny check advisories bans licenses sources` mirrors CI's Rust dependency policy gate. Install the pinned CI version with `cargo install cargo-deny --locked --version 0.19.4` when you need to run it locally.

`cargo xtask verify-release-config` checks that backend AASA identity matches the checked-in iOS team and bundle settings. Use it after changing universal-link identity wiring, AASA payloads, or related iOS project settings.

`cargo xtask verify-stock-ledger` checks that every `stock_batch.quantity` matches the sum of its stock events. Use it after touching `crates/qm-db/src/stock.rs`.

Postgres coverage uses the shared test harness in `qm-db::test_support`:

- `QM_POSTGRES_TEST_URL` points at an existing Postgres server and makes tests create an isolated throwaway database inside it.
- `QM_RUN_POSTGRES_TESTS=1` tells the harness to start its own containerized Postgres instance when available.
- `QM_REQUIRE_POSTGRES_TESTS=1` turns Postgres availability into a hard failure instead of silently skipping those cases.

## API Integration Tests

`qm-api` integration tests live under `crates/qm-api/tests/` and are grouped by behavior: `invites.rs`, `households.rs`, `stock_lifecycle.rs`, and similar. Keep new test files behavior-oriented rather than naming them after implementation phases.

Time and reminder changes need timezone coverage, including household-local semantics and DST edge cases. Cover both SQLite and Postgres behavior where practical.

## iOS

The iOS project is generated from `ios/project.yml` with XcodeGen. Re-run after editing `project.yml` or adding Swift source files:

```sh
cd ios
xcodegen generate
```

After regenerating OpenAPI, rebuild iOS so the build-tool plugin regenerates the Swift client:

```sh
xcodebuild -project ios/Quartermaster.xcodeproj \
  -scheme Quartermaster \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro,OS=26.2' \
  -skipPackagePluginValidation \
  build
```

Run iOS tests with:

```sh
xcodebuild -project ios/Quartermaster.xcodeproj \
  -scheme Quartermaster \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro,OS=26.2' \
  -skipPackagePluginValidation \
  test
```

A warning about `try` on `ok.body.json` accessors is compiler flow analysis noticing the single-case enum never throws. It is harmless generator-side noise.

## Android

From `android/`:

```sh
gradle testDebugUnitTest assembleDebug
```

If generated OpenAPI source wiring changes, verify from a clean app build directory:

```sh
rm -rf app/build
gradle assembleDebug
```

Android host checks need a working JDK, Android SDK, and `android/local.properties`. See [android/README.md](android/README.md) for local emulator setup.

The two known SDK roots are:

- Homebrew command-line tools on Apple Silicon: `/opt/homebrew/share/android-commandlinetools`
- Android Studio's default install: `~/Library/Android/sdk`

Set `sdk.dir` in `android/local.properties` to whichever root owns your installed platforms and build tools.

## Web

The web app uses SvelteKit, TypeScript, pnpm, and a generated TypeScript client from `openapi.json`. Volta pins Node and pnpm in `package.json`; set `VOLTA_FEATURE_PNPM=1` before `pnpm install` in shells or CI environments that need Volta's pnpm shim:

```sh
export VOLTA_FEATURE_PNPM=1
volta install node pnpm
```

```sh
pnpm install --frozen-lockfile
pnpm -C web generate:api
pnpm -C web check
pnpm -C web test
pnpm -C web build
pnpm -C web test:e2e
```

For local development:

```sh
pnpm install
pnpm -C web generate:api
pnpm -C web dev
```

The development server can talk to a local backend by entering `http://localhost:8080` in the web app's server URL field.

The Playwright smoke path builds/uses the static web shell and starts `qm-server` against a temporary SQLite database, so install the Chromium browser first when running it locally:

```sh
pnpm -C web exec playwright install chromium
```

## Formatting Tools

The root `package.json` owns the formatter entry points:

```sh
pnpm format
pnpm format:check
```

Local installs need the language tools those commands call:

- Rust: `rustup component add rustfmt`
- Swift: install `swift-format` and keep it available on `PATH`
- Kotlin: Gradle runs Spotless from the Android build, using the configured SDK/JDK
- Shell: install `shfmt`
- Web and docs-adjacent files: `pnpm install` supplies Prettier through the web workspace

Install commit hooks with `pnpm hooks:install` if you want local pre-commit checks to mirror CI.

## Release Identity And Universal Links

Invite links and app setup codes are built from `QM_PUBLIC_BASE_URL` when it is set. `QM_PUBLIC_BASE_URL` may be an HTTP origin for LAN/self-hosted app setup. For direct app-opening on iOS, use a real HTTPS host: that host must serve `/.well-known/apple-app-site-association`, and the app build must include a matching `applinks:` associated domain. The native app persists the server URL from setup/manual entry and reuses it on later launches.

Quartermaster supports one explicit v1 release identity story:

- one associated-domain host
- one env-driven iOS team ID and bundle ID pairing

Use the wrapper for local release setup:

```sh
cargo xtask configure-release-identity \
  --team YOUR_TEAM_ID \
  --bundle-id com.yourname.Quartermaster \
  --domain quartermaster.example.com
```

It writes the ignored iOS release config and prints the matching `QM_IOS_*` server environment. Keep the associated-domain host aligned with the iOS release identity and use `cargo xtask verify-release-config` as the drift check instead of checking Apple release identity into the repo.

## End-To-End Smoke Test

Start the server:

```sh
cargo run -p qm-server
```

Then register and log in through a client or `curl`, and verify `GET /auth/me` returns `current_household` plus the household membership list. The native apps against a running backend are the real integration test.

If you changed reminder scheduling, reminder delivery, or worker wiring, run `cargo test --workspace` and then do one split-worker smoke test locally:

```sh
cargo run -p qm-server
cargo run -p qm-server -- worker
```

## Project Conventions

- SQL must run against both SQLite and Postgres through `sqlx::Any`; keep queries portable except for deliberate backend-guarded branches such as the Postgres row lock in stock ledger paths.
- Product unit family is fixed across a product's stock batches. Cross-family conversion belongs in recipe concerns, not inventory.
- Stock is event-sourced. Never mutate `stock_batch.quantity` directly; use the stock repository helpers that write events and update the cached quantity in one transaction.
- Database enums are stored as `TEXT`; API DTOs expose typed Rust enums so OpenAPI and generated clients get real enum shapes.
- Recipes, pantry suggestions, and replenishment are review-first surfaces. Keep recipe execution behind preflight/confirmation, and keep cart submission behind explicit guardrail review unless a trusted automation path rechecks policy server-side.
- Authenticated requests resolve one current household per session.
- Invite-backed registration and invite redemption must be transactional and duplicate-safe.
- Expiry dates are household-local calendar dates. Reminder scheduling is household-local policy stored as UTC instants.
- Rust time handling is `jiff`-only. Do not add `chrono` back for DTOs, schema generation, or one-off helpers.
- Internal maintenance hooks stay out of the public OpenAPI contract unless intentionally promoted.
