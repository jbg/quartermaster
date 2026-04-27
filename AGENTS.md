# AGENTS.md

Orientation for anyone — human or AI — landing in this repo. Keep this short; non-obvious conventions only. Don't duplicate what the code or README already says.

See also: [README.md](README.md) for what the project is, and your local `TODO.md` scratchpad for possible follow-up work. `TODO.md` is gitignored by design and is not a shared plan.

## Invariants you can accidentally break

These are enforced in code, but the _why_ lives here. Respect them.

- **A product has one unit family (mass / volume / count).** Every batch of that product is measured in units from that family; the server rejects `CreateStock` / `UpdateStock` otherwise. Cross-family conversion (grams of flour ↔ cups of flour) is a **recipe** concern — we don't attempt it at the inventory layer, because the density depends on the specific product and that's not data we have.
- **Stock is event-sourced.** Every quantity change is a row in `stock_event` (`add` / `consume` / `adjust` / `discard` / `restore`). `stock_batch.quantity` is a _cache_ of `SUM(quantity_delta)` for the batch, maintained inside the same transaction that writes the event. **Never mutate `stock_batch.quantity` directly.** Always go through `qm_db::stock::{adjust, apply_consumption, discard, restore, restore_many}`. Events are kept forever — they're the audit log and the UI's history timeline.
- **TEXT-backed enums at the DB boundary, typed enums at the API boundary.** `product.source`, `product.family`, `stock_event.event_type` are `TEXT` columns (for `sqlx::Any` portability and easy migrations). DTOs convert to typed Rust enums (`ProductSource`, `UnitFamily`, `StockEventType`) at the edge so the OpenAPI spec — and the generated iOS client — gets real enums. Keep this split: don't push enum types into the DB layer.
- **Multiple memberships are allowed; one household is still "current".** Authenticated requests resolve the active household as the membership with the latest `joined_at`; ties break on `household.id DESC` so both SQLite and Postgres choose the same row. `GET /auth/me` and the auth extractor intentionally share this rule.
- **Invite joins are transactional and duplicate-safe.** Invite-backed registration and invite redemption must create membership/user rows and consume the invite in one transaction. Re-redeeming into a household the user already belongs to is an idempotent no-op and must not burn another invite use.
- **Expiry dates are household-local calendar dates.** `stock_batch.expires_on` is interpreted in the household's IANA timezone, not UTC and not the viewer's current device timezone. Editing `household.timezone` is a correction of interpretation only — it must not rewrite stored local dates/datetimes.
- **Reminder scheduling is household-local, storage is UTC.** `QM_EXPIRY_REMINDER_FIRE_HOUR` / `MINUTE` are household-wall-clock settings. Compute the fire instant from `expires_on` + household timezone, then store canonical UTC RFC3339 in `stock_reminder.fire_at`. Keep `household_fire_local_at` / `household_timezone` as metadata; don't reintroduce UTC-based policy interpretation.
- **Reminders are a durable inbox, not fetch-and-disappear alerts.** `GET /reminders` returns due, unacked reminders even after first presentation. Device-local presentation/open/dismissal state lives in `reminder_device_state`; `acked_at` on `stock_reminder` is reserved for household/system-level suppression and lifecycle cleanup, not normal user dismissal. Don't auto-ack on read/poll.
- **Rust time handling is `jiff`-only.** Use `jiff::civil::Date` for date-only values, `jiff::Timestamp` for instants, and the shared helpers in `qm_db::time` for parsing/formatting. Don't add `chrono` back for DTOs, schema generation, or one-off helpers.
- **Operator-only maintenance hooks stay off the public API contract.** Internal endpoints such as `POST /internal/maintenance/sweep-auth-sessions` are deployment plumbing, not app surface. Keep them out of the OpenAPI document and generated mobile clients unless we intentionally promote them into supported product API.
- **JSON Patch support is intentionally narrow.** Product and stock PATCH accept only top-level `replace` and `remove` operations because those map to inventory edit semantics. If future endpoints need array edits, test/move/copy, nested paths, or optimistic concurrency, expand the shared helper deliberately or introduce an endpoint-specific contract.
- **JSON Patch `value` stays loosely typed in OpenAPI.** The schema exposes arbitrary JSON for `value`, which is standard and avoids nullable merge-patch ambiguity. If native client ergonomics suffer, prefer small shared helper aliases/builders over endpoint-specific override DTOs.

## Workflow

- **Regenerate the OpenAPI spec after any DTO, route, or enum change:**
  ```sh
  cargo xtask export-openapi
  ```
  Writes two copies: `openapi.json` (repo-root canonical) and `ios/Quartermaster/openapi.json` (what the Xcode build-tool plugin reads). Commit both.
- **Re-verify release config after touching universal-link identity wiring.**
  ```sh
  cargo xtask verify-release-config
  ```
  Use this after editing the AASA payload, iOS team/bundle identifiers, or `ios/project.yml` settings that affect app-site association identity.
- **Native client DTOs are generated from OpenAPI.** iOS types + `Client` come from [swift-openapi-generator](https://github.com/apple/swift-openapi-generator); Android generates its Retrofit client from the repo-root `openapi.json` during the Gradle build. Don't hand-edit generated DTOs on either client. iOS extensions on generated types live in `ios/Quartermaster/Core/Networking/APIAliases.swift`; the two tri-state PATCH bodies that the Swift generator can't express natively live in `APIOverrides.swift`.
- **After regenerating the spec, rebuild iOS** — the plugin runs during `xcodebuild` / Xcode builds, so changes flow through automatically. First build after a package change may need `-skipPackagePluginValidation`.
- **Keep generated clients behind local helpers.** The web client under `web/src/lib/generated/`, the Android client under `android/app/build/generated/`, and iOS generated types all come from OpenAPI. Don't hand-edit generated output. If generated shapes get awkward, prefer tightening the shared OpenAPI schema over client-specific DTO forks.
- **`xcodegen generate`** (in `ios/`) regenerates the `.xcodeproj` from `project.yml`. Re-run after any `project.yml` edit, and also after adding new Swift source files or source-group structure that Xcode needs to see.
- **Use Conventional Commits for every commit message.** Release Please derives versions from commit subjects: `fix:` for patch, `feat:` for minor, and `type!:` or a `BREAKING CHANGE:` footer for major. Use non-release types like `chore:`, `docs:`, `test:`, or `refactor:` when the change should not bump the product version.
- **Don't edit `CHANGELOG.md` by hand.** Release Please owns changelog updates; leave formatting and release-note edits to the generated release PR.
- **Format before committing.** Run the relevant formatter for your change, or `pnpm format` at the repo root when in doubt. CI enforces Rust (`cargo fmt`), Swift (`swift-format`), Kotlin (`spotless`/`ktlint`), shell (`shfmt`), and web (`prettier`) formatting.

## Verification

- **Rust:** `cargo test --workspace` — fast. Exercises the router, repo layer, unit conversions, and OpenFoodFacts parsing.
- **`qm-api` integration tests are behavior-grouped.** Keep files in `crates/qm-api/tests/` named for the surface they cover (`invites.rs`, `households.rs`, `stock_lifecycle.rs`, …), not for implementation phases or generic "slice" buckets.
- **Time/reminder changes need timezone coverage, not just happy-path UTC assertions.** If you touch reminder scheduling or household timezone handling, add/keep tests for household-local semantics, DST edge cases, and both SQLite/Postgres behavior where practical.
- **Stock-ledger integrity:** `cargo xtask verify-stock-ledger` checks that every `stock_batch.quantity` equals `SUM(stock_event.quantity_delta)` for that batch. Useful after any change in `qm-db/src/stock.rs`.
- **Release-config integrity:** `cargo xtask verify-release-config` checks that the backend AASA `appID` matches the checked-in iOS team + bundle identifier. Useful after any universal-link or signing-identity change.
- **iOS build:** `xcodebuild -project ios/Quartermaster.xcodeproj -scheme Quartermaster -destination 'platform=iOS Simulator,name=iPhone 17 Pro,OS=26.2' -skipPackagePluginValidation build`. A warning about `try` on `ok.body.json` accessors is compiler flow analysis noticing the single-case enum never throws — harmless, generator-side.
- **End-to-end smoke test:** start `cargo run -p qm-server`, register + login via `curl`, verify `GET /auth/me` returns `current_household` plus the household membership list. The native apps against the running backend are the real integration test.
- **Web Playwright smoke is intentionally narrow.** Keep selectors stable when changing screens and update the smoke fixture only when product work would otherwise break the existing flow. Don't add broad web smoke paths just to mirror every affordance.
- **The no-socket mock transport is OFF-specific.** `qm-api/tests/barcode_lookup.rs` is sandbox-safe. If more integration suites need mock upstream HTTP, generalize the `mock://...` pattern or add reusable test transport API.
- **Android smoke tests depend on stable Compose test tags.** `android/scripts/smoke_ui.py` no longer keys critical actions off copy. If Android UI is refactored heavily, keep those tags stable or update the smoke driver in the same slice.
- **Android lifecycle smoke intentionally stays fixture-biased.** Fixture path covers reminder-open targeting, discard/restore, stock correction, product lifecycle, cached barcode lookup, and empty-location lifecycle. Manual-credential smoke should keep avoiding arbitrary user data mutations.
- **The internal smoke fixture route is secret-gated and off-contract.** Keep smoke seeding out of public OpenAPI and generated clients. If local tools need deterministic reminder/invite fixtures, pull fixture assembly into a reusable internal helper.

## Portability

- SQL runs against both SQLite and Postgres via `sqlx::Any`. Keep queries in the portable subset — no SQLite pragmas, no Postgres-specific types. Test against both where practical.
- Concurrent-write safety on Postgres has a known gap documented at the top of `crates/qm-db/src/stock.rs`. Don't paper over it in a drive-by PR — the fix is either `SERIALIZABLE` isolation or `SELECT ... FOR UPDATE` inside the transactional paths.

## Web client rules

- **Web route bootstraps should be one-shot browser mounts.** Inventory and Settings initialize from browser state in `onMount` because reactive `$effect` route initialization caused repeated `/auth/me` and stock-history calls that hit rate limits. New web routes that initialize from `localStorage` or configure the generated API client should follow that pattern.
- **Web product routes are first-class SPA routes.** `/products`, `/products/new`, `/products/{id}`, `/products/{id}/edit`, and `/products/{id}/delete` are refresh-safe frontend routes. Because adapter-static cannot prerender arbitrary product IDs, `svelte.config.js` ignores unseen routes and the Rust web fallback serves `200.html`; keep future dynamic web routes on that model unless they need server ownership.
- **Web inventory date filters are browser-local today.** Expiring-soon/expired filtering uses the browser's local date, not the household timezone. Preserve that behavior unless the feature is intentionally changed end-to-end.
- **Backend static serving is deliberately conservative.** Rust serves `/_app`, `/`, `/join`, and non-API GET fallbacks from `QM_WEB_DIST_DIR`; API paths under `/api/v1`, docs, health, well-known, and internal hooks remain owned by Axum routes. New frontend routes should use SPA fallback unless they truly need server-rendered HTML.

## Hosted and release rules

- **Reverse-proxy trust is explicit.** Trusted CIDRs are required before honoring `X-Forwarded-For`. Don't loosen that without a deliberate replacement model such as `Forwarded` header support or trusted-hop handling.
- **Home Assistant options are intentionally whitelisted.** The container entrypoint reads only current non-secret app options from `/data/options.json`. If the HA app grows APNs/FCM credentials, maintenance secrets, reverse-proxy settings, or push-worker mode, add those deliberately and avoid logging secrets.
- **The HA app repo owns its own metadata.** `scripts/update-ha-app-repo.sh` edits the checked-out app repo in place instead of copying a template. Keep `quartermaster/config.yaml`, docs, icons, option schema, and validation workflow in `jbg/quartermaster-home-assistant-apps`; the main repo should only bump `version` and prepend changelog notes.
- **Release identity is env-driven and split by surface.** `QM_IOS_*` configures the server AASA payload and `QUARTERMASTER_IOS_*` configures the app build. Use `cargo xtask verify-release-config` as the drift check instead of committing official Apple identity to the repo.

## iOS client quirks worth knowing

- **IDs are `Swift.String`, not `Foundation.UUID`.** swift-openapi-generator treats `format: uuid` as an annotation on a string. Views pass IDs around as strings; parse to `UUID` only when the domain actually needs it.
- **`namingStrategy: idiomatic`** converts `snake_case` schema fields to `camelCase` Swift properties. Abbreviations lose their capital letters: `image_url` → `imageUrl`, `location_id` → `locationId`, `next_before_id` → `nextBeforeId`. `APIAliases.swift` exposes old-style names (`imageURL`, `locationID`, …) as computed aliases so feature views don't have to rename.
- **If a Rust DTO needs an optional nested object, prefer an explicit OpenAPI schema over generator-specific flattening.** `GET /auth/me` now exposes `current_household` as a nullable object via a manual `ToSchema`/`PartialSchema` shape so both iOS and Android can consume the same contract cleanly. If another optional complex field appears, keep the wire contract clean and fix the schema generation rather than introducing client-specific DTO forks.
- **Keep new household-scoped iOS surfaces on the shared recovery path.** Inventory, Settings, and household history go through one `AppState` helper for `403` recovery instead of open-coding `/auth/me` refresh logic. If more household-root screens appear, use that same contract.
- **Keep new household-entry points on the shared action surface.** No Household and Settings share one switch/redeem/create controller/component. If more entry points appear, compose that surface instead of cloning local state and action code.

## Style

- Small, focused PRs over big ones.
- Comments explain _why_, not _what_. Well-named code is the _what_.
- Don't add error handling for impossible cases. Trust internal invariants; validate at system boundaries (user input, OFF responses).
- Prefer editing existing files over creating new ones.
- Keep formatter/tool-version churn separate from behavior changes; formatter baselines are intentionally broad.
