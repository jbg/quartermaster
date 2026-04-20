# Quartermaster — next slice candidates

Grouped by theme, not priority. The natural order to finish v1 is roughly **Slice 2 follow-ups → Slice 3 → cross-cutting**.

Slice 2 (products + barcodes + stock + scan + populated inventory) shipped in `4aa83c0`.

## Slice 2 follow-ups — things we noticed while building

Smaller than a slice of their own, but they'll bite if left out. Roughly ordered by impact.

- **Product edit (name, brand, family, preferred unit, image).** OpenFoodFacts' family inference falls back to `count` when it can't tell, and occasionally gets it wrong outright (Coca-Cola landed as `mass` in testing because OFF's `product_quantity_unit` was "g" for some SKUs). Users need a way to correct a product's family and preferred unit after the fact. Also needed for fixing typos in manual products. `PATCH /products/{id}` (admin-or-creator) + an iOS product detail screen. Deleting a product that has stock should be refused.
- **Move a batch between locations.** `EditBatchForm` currently only presents the batch's current location, so "I just moved this from the pantry to the freezer" is awkward. Fix: pass all `locations` into `EditBatchForm` (already in scope on the caller) and bind the picker.
- **Expose `opened_on` in AddStock + batch edit.** The DB column, DTO, and request type all carry it; the iOS forms don't show it yet. Useful for dairy, sauces, and anything with a "use within N days once opened" expectation.
- **Explicit "clear expiry" control on iOS.** Backend supports `null` via double-option. The edit form only lets you set a date or leave it alone — add a "Remove expiry" destructive button inside the expiry section.
- **Make filter-chip totals honest.** Under *Expiring soon*, a product row shows the sum of only the matching batches and the filtered batch count. That's logically consistent but probably confusing ("I have 1.3 kg total but this says 300 g?"). Options: (a) show both "300 g expiring soon / 1.3 kg total" on the row, or (b) hide the product row from the section when any of its batches don't match — i.e. surface only *fully* expiring-soon products. Worth a product call.
- **Force-refresh a cached barcode.** Today the TTL is the only path back to OFF once we've cached a response. Add a "Refresh from OpenFoodFacts" action on product detail that invalidates the cache row and re-fetches — useful when OFF data improves or our inference was wrong.
- **Consume response in the requested unit.** `POST /stock/consume` returns quantities in each *batch's* unit (so consuming 400 ml across a 500 ml + 1 l batch pair comes back as "300 ml / 0.10 l"). Technically correct, UX-wise odd. Add a `requested_unit`-normalised quantity to each `ConsumedBatchDto` alongside the batch's own.
- **Decimal-pad keyboard dismissal.** iOS decimal pad has no return key. Add a toolbar "Done" to quantity fields in AddStockView / EditBatchForm / ConsumeForm, or dismiss on tap-outside.
- **Depleted batch retention policy.** `stock_batch` rows with `depleted_at` set are kept forever. Not a problem at household scale but there's no endpoint or UI to see/clean them. Either add a retention window (delete after 1 year?) or a "History" screen gated behind a "Show depleted" toggle on `/stock`.
- **Default family picker on manual product.** Currently defaults to `Count`. Mass is probably more common for manual entries (bulk staples, spices). Trivial default change but worth validating.

## Slice 3 — household management + admin polish

Needed before any multi-user self-hosted deployment makes sense.

- **Invite codes.** `POST /households/current/invites` to mint (admin only), `GET /households/current/invites` to list, `DELETE /invites/{id}` to revoke, `POST /invites/redeem` for existing users, and the `invite_code` path of `POST /auth/register` (currently returns `BadRequest` — see `crates/qm-api/src/routes/accounts.rs`).
- **Locations CRUD beyond GET.** `POST /locations`, `PATCH /locations/{id}` (rename, reorder), `DELETE /locations/{id}` with a check that no non-depleted `stock_batch` references it.
- **Members management.** `GET /households/current/members`, `DELETE /households/current/members/{user_id}` (admin only, prevent last-admin removal).
- **Registration modes.** Implement `invite_only` and `open` fully in `POST /auth/register`; today only `first_run_only` works end-to-end.
- **iOS Settings detail.** Household rename, invite code list + share sheet, member list with role badges.

## Cross-cutting — polish and plumbing

Not blocking v1 but the earlier each lands the better the rest of the work feels.

- **swift-openapi-generator SPM plugin.** Replace hand-rolled DTOs in `ios/Quartermaster/Core/Networking/APIDTOs.swift` with generated types driven off the checked-in `openapi.json`. Add the plugin via `project.yml` and regenerate on every `cargo xtask export-openapi`. With slice 2 done, the DTO surface is ~10 types — the right time to cut over before it grows further.
- **Postgres support in qm-db tests.** Spin up Postgres via testcontainers; run the same repo tests against both backends to catch `sqlx::Any` divergences early.
- **Integration tests in qm-api.** Spin up the router against a temp SQLite DB and exercise the full auth + products + stock flow in Rust — currently only covered by the manual curl script used to verify each slice.
- **Docker image + compose quickstart.** Static-linked release binary in a `scratch` or `distroless` image; a `docker-compose.yml` with the SQLite volume mount and env vars documented in README.
- **GitHub Actions CI.** `cargo test --workspace`, `cargo xtask export-openapi` and fail if `openapi.json` drifts, `xcodebuild -scheme Quartermaster build` on a hosted macOS runner. Matrix the Rust job over SQLite and Postgres.
- **Structured tracing.** JSON log output behind `RUST_LOG=info,qm=debug`; request IDs in spans; include user_id/household_id in request scopes once auth is resolved.
- **Rate limiting on auth + barcode endpoints.** `tower-governor` or similar on `/auth/login`, `/auth/register`, `/auth/refresh` to slow down credential stuffing; also on `/products/by-barcode/{}` so a single misbehaving client can't batter OpenFoodFacts on our behalf.
- **OFF client hardening.** Today a single request = single `reqwest` call with a 5s timeout. Add retries with jitter on transient upstream errors, and a circuit breaker so a prolonged OFF outage doesn't stall every barcode scan for 5 seconds.
- **ODbL audit for barcode cache.** Before a hosted deployment exists, determine whether the `barcode_cache` table as used constitutes a derived database under OpenFoodFacts' licence and document obligations (attribution, share-alike if redistributing).
- **Expiry push notifications.** The whole "expiring soon" UX is strongest with proactive reminders. Local notifications (scheduled on the device when a batch is added) get us most of the way; APNs becomes useful if we want household-wide nudges.
- **Android shell.** Kotlin + Jetpack Compose, mirroring the iOS tab shape. Same OpenAPI spec → Kotlin client via `openapi-generator`.
- **Web shell.** Lowest priority per the platform ordering. Likely SvelteKit or Next.js against the same OpenAPI spec; can piggy-back on the existing auth surface.
