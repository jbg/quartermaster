# Quartermaster

A mobile-first, self-hostable kitchen inventory management system.

Quartermaster tells you what's in your kitchen, what's about to expire, and lets you add stock quickly by scanning a barcode. That's it. No recipes, no meal planning, no to-do lists — at least not for now. It's a leaner, inventory-focused alternative to [Grocy](https://grocy.info).

## Status

Early work in progress. v1 is being built toward an "empty pantry" first vertical slice.

## Architecture

- **Backend:** Rust (Axum + SQLx + Tokio), single self-hosted binary
- **Database:** SQLite by default (one `.db` file), Postgres optional via config
- **Mobile:** native clients. iOS first (SwiftUI, iOS 26, Liquid Glass), then Android, then web. iOS types + HTTP client are generated at Xcode build time from the checked-in OpenAPI spec via [swift-openapi-generator](https://github.com/apple/swift-openapi-generator)
- **Products / barcodes:** [OpenFoodFacts](https://world.openfoodfacts.org) with local cache; manual entry always available
- **Auth:** local accounts with household invite codes; opaque access + refresh tokens
- **Households:** users may belong to multiple households; the active one is the most recently joined membership (with `household.id` as the deterministic tiebreak when timestamps match)
- **License:** Apache-2.0

## Repository layout

```
.
├── Cargo.toml              workspace manifest
├── crates/
│   ├── qm-core/            domain logic (units, batches, errors) — no I/O
│   ├── qm-db/              SQLx repos + migrations
│   ├── qm-api/             Axum handlers, middleware, OpenAPI
│   └── qm-server/          the shipped binary
├── xtask/                  developer tasks (export-openapi, …)
├── openapi.json            generated spec (canonical copy, for external consumers + CI drift check)
└── ios/Quartermaster/      SwiftUI app — consumes openapi.json via a build-tool plugin
    └── openapi.json        second copy, read by the Xcode plugin (kept in sync by xtask)
```

## Running the backend

```sh
cargo run -p qm-server
```

The server listens on `0.0.0.0:8080` and creates `data.db` in the working directory by default. Override with environment variables:

| Variable                | Default                     | Meaning                                      |
|-------------------------|-----------------------------|----------------------------------------------|
| `QM_BIND`               | `0.0.0.0:8080`              | Bind address                                 |
| `QM_DATABASE_URL`       | `sqlite://data.db?mode=rwc` | SQLx connection string (SQLite or Postgres)  |
| `QM_REGISTRATION_MODE`  | `first_run_only`            | `first_run_only` \| `invite_only` \| `open`  |
| `RUST_LOG`              | `info`                      | Tracing filter                               |

Then probe it:

```sh
curl http://localhost:8080/healthz
open http://localhost:8080/docs      # Swagger UI (when built with default features)
```

## Regenerating the OpenAPI spec

```sh
cargo xtask export-openapi
```

Writes `openapi.json` at the repo root **and** at `ios/Quartermaster/openapi.json`. The iOS target's Xcode build-tool plugin reads the second copy; the first is the canonical one (external consumers, CI drift check). Commit both so the iOS build stays hermetic.

Re-run this after any change to a Rust DTO, route, or enum — the next iOS build will regenerate `Components.Schemas.*` and the generated `Client` automatically.

Invite-backed registration and `POST /invites/redeem` are transactional: creating the user/membership and consuming the invite happen together, and redeeming an invite for a household the user already belongs to is treated as an idempotent no-op rather than consuming another use.

## Contributing

The v1 scope is intentionally narrow — see the status section. Please open an issue to discuss feature ideas before writing code.

## Open Food Facts & ODbL

Barcode lookups hit the [Open Food Facts](https://world.openfoodfacts.org) public API, and the server caches the result locally in `barcode_cache`. Open Food Facts data is licensed under the [Open Database Licence (ODbL) v1.0](https://opendatacommons.org/licenses/odbl/1-0/). That has implications for anyone *redistributing* a Quartermaster instance's database (self-hosters who only use it privately are fine). A more thorough audit is tracked in [TODO.md](TODO.md) under cross-cutting work.

## License

Apache License 2.0 — see [LICENSE](LICENSE).
