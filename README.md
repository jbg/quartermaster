# Quartermaster

A mobile-first, self-hostable kitchen inventory management system.

Quartermaster tells you what's in your kitchen, what's about to expire, and lets you add stock quickly by scanning a barcode. That's it. No recipes, no meal planning, no to-do lists — at least not for now. It's a leaner, inventory-focused alternative to [Grocy](https://grocy.info).

## Status

Early work in progress. v1 is being built toward an "empty pantry" first vertical slice.

## Architecture

- **Backend:** Rust (Axum + SQLx + Tokio), single self-hosted binary
- **Database:** SQLite by default (one `.db` file), Postgres optional via config
- **Mobile:** native clients. iOS first (SwiftUI, iOS 26, Liquid Glass), then Android, then web
- **Products / barcodes:** [OpenFoodFacts](https://world.openfoodfacts.org) with local cache; manual entry always available
- **Auth:** local accounts with household invite codes; opaque access + refresh tokens
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
├── openapi.json            generated, checked in for hermetic mobile builds
└── ios/Quartermaster/      SwiftUI app
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

Writes `openapi.json` at the repo root. Commit the result so the iOS build plugin stays hermetic.

## Contributing

The v1 scope is intentionally narrow — see the status section. Please open an issue to discuss feature ideas before writing code.

## Open Food Facts & ODbL

Barcode lookups hit the [Open Food Facts](https://world.openfoodfacts.org) public API, and the server caches the result locally in `barcode_cache`. Open Food Facts data is licensed under the [Open Database Licence (ODbL) v1.0](https://opendatacommons.org/licenses/odbl/1-0/). That has implications for anyone *redistributing* a Quartermaster instance's database (self-hosters who only use it privately are fine). A more thorough audit is tracked in [TODO.md](TODO.md) under cross-cutting work.

## License

Apache License 2.0 — see [LICENSE](LICENSE).
