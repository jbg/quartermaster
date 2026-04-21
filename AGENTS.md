# AGENTS.md

Orientation for anyone — human or AI — landing in this repo. Keep this short; non-obvious conventions only. Don't duplicate what the code or README already says.

See also: [README.md](README.md) for what the project is, [TODO.md](TODO.md) for what's next (gitignored — your local scratchpad, not a shared plan).

## Invariants you can accidentally break

These are enforced in code, but the *why* lives here. Respect them.

- **A product has one unit family (mass / volume / count).** Every batch of that product is measured in units from that family; the server rejects `CreateStock` / `UpdateStock` otherwise. Cross-family conversion (grams of flour ↔ cups of flour) is a **recipe** concern — we don't attempt it at the inventory layer, because the density depends on the specific product and that's not data we have.
- **Stock is event-sourced.** Every quantity change is a row in `stock_event` (`add` / `consume` / `adjust` / `discard` / `restore`). `stock_batch.quantity` is a *cache* of `SUM(quantity_delta)` for the batch, maintained inside the same transaction that writes the event. **Never mutate `stock_batch.quantity` directly.** Always go through `qm_db::stock::{adjust, apply_consumption, discard, restore, restore_many}`. Events are kept forever — they're the audit log and the UI's history timeline.
- **TEXT-backed enums at the DB boundary, typed enums at the API boundary.** `product.source`, `product.family`, `stock_event.event_type` are `TEXT` columns (for `sqlx::Any` portability and easy migrations). DTOs convert to typed Rust enums (`ProductSource`, `UnitFamily`, `StockEventType`) at the edge so the OpenAPI spec — and the generated iOS client — gets real enums. Keep this split: don't push enum types into the DB layer.

## Workflow

- **Regenerate the OpenAPI spec after any DTO, route, or enum change:**
  ```sh
  cargo xtask export-openapi
  ```
  Writes two copies: `openapi.json` (repo-root canonical) and `ios/Quartermaster/openapi.json` (what the Xcode build-tool plugin reads). Commit both.
- **iOS types + `Client` are generated** from that second copy by [swift-openapi-generator](https://github.com/apple/swift-openapi-generator). Don't hand-edit `Components.Schemas.*`; don't add DTO structs to a Swift file. Extensions on generated types live in `ios/Quartermaster/Core/Networking/APIAliases.swift`; the two tri-state PATCH bodies that the generator can't express natively live in `APIOverrides.swift`.
- **After regenerating the spec, rebuild iOS** — the plugin runs during `xcodebuild` / Xcode builds, so changes flow through automatically. First build after a package change may need `-skipPackagePluginValidation`.
- **`TODO.md` is gitignored by design.** Treat it as a personal scratchpad for the current working session. Don't refer to it from tracked code or docs.
- **`xcodegen generate`** (in `ios/`) regenerates the `.xcodeproj` from `project.yml`. Re-run after any `project.yml` edit.

## Verification

- **Rust:** `cargo test --workspace` — fast. Exercises the router, repo layer, unit conversions, and OpenFoodFacts parsing.
- **Stock-ledger integrity:** `cargo xtask verify-stock-ledger` checks that every `stock_batch.quantity` equals `SUM(stock_event.quantity_delta)` for that batch. Useful after any change in `qm-db/src/stock.rs`.
- **iOS build:** `xcodebuild -project ios/Quartermaster.xcodeproj -scheme Quartermaster -destination 'platform=iOS Simulator,name=iPhone 17 Pro,OS=26.2' -skipPackagePluginValidation build`. A warning about `try` on `ok.body.json` accessors is compiler flow analysis noticing the single-case enum never throws — harmless, generator-side.
- **End-to-end smoke test:** start `cargo run -p qm-server`, register + login via `curl`, verify `GET /auth/me` returns `household_id` + `household_name`. The iOS app against the running backend is the real integration test.

## Portability

- SQL runs against both SQLite and Postgres via `sqlx::Any`. Keep queries in the portable subset — no SQLite pragmas, no Postgres-specific types. Test against both (one is CI-shaped, the other is a gap — see `TODO.md`).
- Concurrent-write safety on Postgres has a known gap documented at the top of `crates/qm-db/src/stock.rs`. Don't paper over it in a drive-by PR — the fix is either `SERIALIZABLE` isolation or `SELECT ... FOR UPDATE` inside the transactional paths.

## iOS client quirks worth knowing

- **IDs are `Swift.String`, not `Foundation.UUID`.** swift-openapi-generator treats `format: uuid` as an annotation on a string. Views pass IDs around as strings; parse to `UUID` only when the domain actually needs it.
- **`namingStrategy: idiomatic`** converts `snake_case` schema fields to `camelCase` Swift properties. Abbreviations lose their capital letters: `image_url` → `imageUrl`, `location_id` → `locationId`, `next_before_id` → `nextBeforeId`. `APIAliases.swift` exposes old-style names (`imageURL`, `locationID`, …) as computed aliases so feature views don't have to rename.
- **`Option<ComplexType>` on a Rust DTO field emits `oneOf: [null, $ref]`**, which swift-openapi-generator silently drops. Either flatten to primitive optionals (`Option<Uuid>`, `Option<String>`) or write a manual `ToSchema` impl. `MeResponse.household_id` / `household_name` is the current example.

## Style

- Small, focused PRs over big ones.
- Comments explain *why*, not *what*. Well-named code is the *what*.
- Don't add error handling for impossible cases. Trust internal invariants; validate at system boundaries (user input, OFF responses).
- Prefer editing existing files over creating new ones.
