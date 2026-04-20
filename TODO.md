# Quartermaster — next slice candidates

Grouped by theme, not priority. The natural order to finish v1 is roughly **Event-log follow-ups → Slice 3 → cross-cutting**.

Shipped so far:
- Slice 1 — initial skeleton (`5263a83`)
- Slice 2 — products, barcodes, stock, scan, populated inventory (`4aa83c0`)
- Slice 2 follow-ups — event-sourced stock, product management, UX polish (`e8666c2`)

## Event-log follow-ups — things we noticed after making stock event-sourced

The ledger exists but is inert from the user's perspective — it's written on every change and never read outside tests. Small bits of scaffolding turn it into the consumption-history / analytics foundation the refactor was meant to unlock.

- **`GET /stock/events` + `GET /stock/{id}/events`.** Read-side HTTP surface over the `stock_event` table. Paginated by `created_at DESC`, filterable by `event_type` and `since=`. Keep it stable so iOS and future clients can build timeline views against it. Reuses `qm_db::stock_events::list_for_household` which already exists.
- **`cargo xtask verify-stock-ledger`.** Walk every `stock_batch` and assert `quantity == SUM(stock_event.quantity_delta)`. Fast, run-on-demand — catches silent drift if a bug ever writes one side of the transaction without the other. A CI pre-flight hook could run it against a fresh migration + fixture.
- **iOS "History" sheet.** Minimal first pass: a "History" row in Settings (or on the batches sheet) that opens a timeline — date, product, event type, delta, and who did it. Depleted batches finally get a home. No analytics yet; just the raw events sorted by time.
- **Undo a discard.** Deleting a batch is currently an accepted one-way trip. The ledger can support a "restore" event (`delta = +previous_current_quantity`, `event_type = restore`, clears `depleted_at`). Tie it to a swipe action on a history row or a snackbar "Undo" immediately after delete.
- **Refresh should respect family conflicts.** `POST /products/{id}/refresh` currently trusts whatever OFF returns and overwrites `family`. If a household has active batches whose units don't fit the new family, we'd silently corrupt referential integrity. Mirror the check from `PATCH /products/{id}`: if the incoming family differs from the current one, run `conflicting_units_for_family_change` and refuse with `product_has_incompatible_stock`.
- **Concurrent-adjust race.** Two users editing the same batch at the same second both read the pre-change quantity, compute deltas against it, and each writes an event + updates the cached balance. SQLite's default serialisable transactions save us today; Postgres under READ COMMITTED would not. Either run `UPDATE … RETURNING` to fold the select-for-compute into the write, or set Postgres connections to `SERIALIZABLE` isolation. Document the assumption either way.
- **Surface `quantity_in_requested_unit` in the iOS consume flow.** Server returns it; nothing displays it. A "Consumed 400 ml across two batches" confirmation toast / sheet is the natural home.
- **Un-delete a product.** Soft-deleted manual products stay in the DB forever. No UI to resurrect. Small admin screen or a "Show deleted" toggle on product search would cover it.
- **Image URL on manual products.** `UpdateProductRequest` accepts `image_url`, but `ProductDetailView` doesn't expose it. Not a barn-burner — a URL field plus a basic remote-image picker would close it out.

## Slice 3 — household management + admin polish

Needed before any multi-user self-hosted deployment makes sense.

- **Invite codes.** `POST /households/current/invites` to mint (admin only), `GET /households/current/invites` to list, `DELETE /invites/{id}` to revoke, `POST /invites/redeem` for existing users, and the `invite_code` path of `POST /auth/register` (currently returns `BadRequest` — see `crates/qm-api/src/routes/accounts.rs`).
- **Locations CRUD beyond GET.** `POST /locations`, `PATCH /locations/{id}` (rename, reorder), `DELETE /locations/{id}` with a check that no non-depleted `stock_batch` references it.
- **Members management.** `GET /households/current/members`, `DELETE /households/current/members/{user_id}` (admin only, prevent last-admin removal).
- **Registration modes.** Implement `invite_only` and `open` fully in `POST /auth/register`; today only `first_run_only` works end-to-end.
- **iOS Settings detail.** Household rename, invite code list + share sheet, member list with role badges.

## Cross-cutting — polish and plumbing

Not blocking v1 but the earlier each lands the better the rest of the work feels.

- **swift-openapi-generator SPM plugin.** Replace hand-rolled DTOs in `ios/Quartermaster/Core/Networking/APIDTOs.swift` with generated types driven off the checked-in `openapi.json`. Add the plugin via `project.yml` and regenerate on every `cargo xtask export-openapi`. The DTO surface is ~15 types now — past the right moment to cut over, but still worth doing before Slice 3 piles on more.
- **Postgres support in qm-db tests.** Spin up Postgres via testcontainers; run the same repo tests against both backends to catch `sqlx::Any` divergences early. Especially important now that `apply_consumption` / `adjust` / `discard` rely on transactional semantics.
- **Integration tests in qm-api.** Spin up the router against a temp SQLite DB and exercise the full auth + products + stock + events flow in Rust — currently only covered by the manual curl script used to verify each slice.
- **Docker image + compose quickstart.** Static-linked release binary in a `scratch` or `distroless` image; a `docker-compose.yml` with the SQLite volume mount and env vars documented in README.
- **GitHub Actions CI.** `cargo test --workspace`, `cargo xtask export-openapi` and fail if `openapi.json` drifts, `cargo xtask verify-stock-ledger` against a fixture DB, `xcodebuild -scheme Quartermaster build` on a hosted macOS runner. Matrix the Rust job over SQLite and Postgres.
- **Structured tracing.** JSON log output behind `RUST_LOG=info,qm=debug`; request IDs in spans; include user_id/household_id in request scopes once auth is resolved.
- **Rate limiting on auth + barcode endpoints.** `tower-governor` or similar on `/auth/login`, `/auth/register`, `/auth/refresh` to slow down credential stuffing; also on `/products/by-barcode/{}` so a single misbehaving client can't batter OpenFoodFacts on our behalf.
- **OFF client hardening.** Today a single request = single `reqwest` call with a 5s timeout. Add retries with jitter on transient upstream errors, and a circuit breaker so a prolonged OFF outage doesn't stall every barcode scan for 5 seconds.
- **ODbL audit for barcode cache.** Before a hosted deployment exists, determine whether the `barcode_cache` table as used constitutes a derived database under OpenFoodFacts' licence and document obligations (attribution, share-alike if redistributing).
- **Expiry push notifications.** The whole "expiring soon" UX is strongest with proactive reminders. Local notifications (scheduled on the device when a batch is added) get us most of the way; APNs becomes useful if we want household-wide nudges.
- **Android shell.** Kotlin + Jetpack Compose, mirroring the iOS tab shape. Same OpenAPI spec → Kotlin client via `openapi-generator`.
- **Web shell.** Lowest priority per the platform ordering. Likely SvelteKit or Next.js against the same OpenAPI spec; can piggy-back on the existing auth surface.
