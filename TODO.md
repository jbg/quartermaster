# Quartermaster — next slice candidates

Grouped by theme, not priority. The natural order to finish v1 is roughly **Slice 2 → Slice 3 → cross-cutting**, because Slice 2 closes out the MVP feature set and everything else is supporting work around it.

## Slice 2 — the rest of the MVP

Once these land, v1 is feature-complete per the build plan: you can add stock by scanning a barcode, track expiry, and consume FIFO.

- **Product model + OpenFoodFacts proxy.** Wire up `product` and `barcode_cache` tables (schema already exists). `GET /products/by-barcode/{barcode}` hits the cache, falls back to OFF's public API, writes hit-or-miss back with appropriate TTLs, returns 404 with a `not_found` code when OFF also has no match so the client can fall through to manual entry. `POST /products` for manual creation, `GET /products/search?q=` for name lookup.
- **Stock batch endpoints.** `GET /stock?location_id=&product_id=&expiring_before=` for inventory views, `POST /stock` to add a batch, `PATCH /stock/{id}` to correct quantity/expiry/location, `DELETE /stock/{id}` to discard, `POST /stock/consume` to plan + apply FIFO consumption via `qm-core::batch::plan_consumption` (already tested).
- **iOS Scan tab.** Replace the placeholder with `DataScannerViewController` (VisionKit) wrapped in `UIViewControllerRepresentable`; detected barcode pushes an `AddStockFlow` sheet with product prefilled when found.
- **iOS AddStock flow.** Form with product (search or scan), quantity + unit picker (units pulled from `qm-core`'s table via a `GET /units` endpoint or hardcoded client-side), location, optional expiry date, optional note. Submits to `POST /stock`.
- **iOS Inventory populated state.** Replace the per-location `ContentUnavailableView` with a real list of batches grouped by product, earliest expiry first. Tap to open a batch detail sheet for editing/deleting.
- **"Expiring soon" view.** Either a filter on Inventory or a separate tab section driven by `expiring_before=` query. Highlight anything expired or within 3 days.

## Slice 3 — household management + admin polish

Needed before any multi-user self-hosted deployment makes sense.

- **Invite codes.** `POST /households/current/invites` to mint (admin only), `GET /households/current/invites` to list, `DELETE /invites/{id}` to revoke, `POST /invites/redeem` for existing users, and the `invite_code` path of `POST /auth/register` (currently returns `BadRequest` — see `crates/qm-api/src/routes/accounts.rs`).
- **Locations CRUD beyond GET.** `POST /locations`, `PATCH /locations/{id}` (rename, reorder), `DELETE /locations/{id}` with a check that no non-depleted `stock_batch` references it.
- **Members management.** `GET /households/current/members`, `DELETE /households/current/members/{user_id}` (admin only, prevent last-admin removal).
- **Registration modes.** Implement `invite_only` and `open` fully in `POST /auth/register`; today only `first_run_only` works end-to-end.
- **iOS Settings detail.** Household rename, invite code list + share sheet, member list with role badges.

## Cross-cutting — polish and plumbing

Not blocking v1 but the earlier each lands the better the rest of the work feels.

- **swift-openapi-generator SPM plugin.** Replace hand-rolled DTOs in `ios/Quartermaster/Core/Networking/APIDTOs.swift` with generated types driven off the checked-in `openapi.json`. Add the plugin via `project.yml` and regenerate on every `cargo xtask export-openapi`.
- **Postgres support in qm-db tests.** Spin up Postgres via testcontainers; run the same repo tests against both backends to catch `sqlx::Any` divergences early.
- **Integration tests in qm-api.** Spin up the router against a temp SQLite DB and exercise the full auth + locations flow in Rust — mirrors the curl script used to verify the vertical slice.
- **Docker image + compose quickstart.** Static-linked release binary in a `scratch` or `distroless` image; a `docker-compose.yml` with the SQLite volume mount and env vars documented in README.
- **GitHub Actions CI.** `cargo test --workspace`, `cargo xtask export-openapi` and fail if `openapi.json` drifts, `xcodebuild -scheme Quartermaster build` on a hosted macOS runner. Matrix the Rust job over SQLite and Postgres.
- **Structured tracing.** JSON log output behind `RUST_LOG=info,qm=debug`; request IDs in spans; include user_id/household_id in request scopes once auth is resolved.
- **Rate limiting on auth endpoints.** `tower-governor` or similar on `/auth/login`, `/auth/register`, `/auth/refresh` to slow down credential stuffing against self-hosted instances.
- **ODbL audit for barcode cache.** Before a hosted deployment exists, determine whether the `barcode_cache` table as used constitutes a derived database under OpenFoodFacts' licence and document obligations (attribution, share-alike if redistributing).
- **Android shell.** Kotlin + Jetpack Compose, mirroring the iOS tab shape. Same OpenAPI spec → Kotlin client via `openapi-generator`.
- **Web shell.** Lowest priority per the platform ordering. Likely SvelteKit or Next.js against the same OpenAPI spec; can piggy-back on the existing auth surface.
