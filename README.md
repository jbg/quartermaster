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
│   │   └── tests/          integration tests grouped by behavior (auth, invites, households, stock, products, request IDs, barcode lookup)
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
| `QM_LOG_FORMAT`         | `text`                      | Log formatter: `text` or `json`              |
| `QM_REGISTRATION_MODE`  | `first_run_only`            | `first_run_only` \| `invite_only` \| `open`  |
| `QM_PUBLIC_BASE_URL`    | unset                       | Public URL used in invite/share links        |
| `RUST_LOG`              | `info`                      | Tracing filter                               |

Then probe it:

```sh
curl http://localhost:8080/healthz
open http://localhost:8080/docs      # Swagger UI (when built with default features)
```

Every HTTP response includes an `X-Request-Id` header. If a client supplies one, Quartermaster propagates it; otherwise the server generates one. Authenticated request spans also record the resolved `user_id` and `household_id`, and `QM_LOG_FORMAT=json` switches logs to newline-delimited JSON for structured ingestion.

Users may belong to multiple households. The active household is session-scoped: each logged-in device/session keeps its own selected household, and `POST /auth/switch-household` changes that selection for the current session only.

Quartermaster also supports a few self-hosting hardening knobs:

| Variable                                   | Default                                           | Meaning |
|--------------------------------------------|---------------------------------------------------|---------|
| `QM_RATE_LIMIT_CLIENT_IP_MODE`            | `socket`                                          | `socket` for direct client IPs, or `x-forwarded-for` when a trusted reverse proxy rewrites that header |
| `QM_RATE_LIMIT_AUTH_PER_MINUTE`            | `10`                                              | Per-client auth request refill rate |
| `QM_RATE_LIMIT_AUTH_BURST`                 | `5`                                               | Per-client auth burst bucket size |
| `QM_RATE_LIMIT_BARCODE_PER_MINUTE`         | `60`                                              | Per-client barcode lookup refill rate |
| `QM_RATE_LIMIT_BARCODE_BURST`              | `20`                                              | Per-client barcode lookup burst bucket size |
| `QM_RATE_LIMIT_HISTORY_PER_MINUTE`         | `120`                                             | Per-client history request refill rate |
| `QM_RATE_LIMIT_HISTORY_BURST`              | `40`                                              | Per-client history burst bucket size |
| `QM_OFF_API_BASE_URL`                      | `https://world.openfoodfacts.org/api/v2/product`  | Open Food Facts API base URL |
| `QM_PUBLIC_BASE_URL`                       | unset                                             | Public base URL for invite/share links |
| `QM_OFF_TIMEOUT_SECONDS`                   | `5`                                               | Timeout for one OFF HTTP request |
| `QM_OFF_MAX_RETRIES`                       | `2`                                               | Retries for transient OFF failures |
| `QM_OFF_RETRY_BASE_DELAY_MS`               | `200`                                             | Base backoff delay for OFF retries |
| `QM_OFF_CIRCUIT_BREAKER_FAILURE_THRESHOLD` | `5`                                               | Consecutive transient OFF failures before opening the breaker |
| `QM_OFF_CIRCUIT_BREAKER_OPEN_SECONDS`      | `60`                                              | How long OFF stays fail-fast once the breaker opens |

Keep `QM_RATE_LIMIT_CLIENT_IP_MODE=socket` for direct deployments or simple local setups. Switch to `x-forwarded-for` only when Quartermaster sits behind a trusted reverse proxy that overwrites `X-Forwarded-For`; otherwise a client could spoof its own rate-limit identity.

## Tests

`cargo test --workspace` is the default fast verification pass.

Postgres coverage uses the shared test harness in `qm-db::test_support`:

- `QM_POSTGRES_TEST_URL` points at an existing Postgres server and makes the tests create an isolated throwaway database inside it.
- `QM_RUN_POSTGRES_TESTS=1` tells the harness to start its own containerized Postgres instance when available locally.
- `QM_REQUIRE_POSTGRES_TESTS=1` turns Postgres availability into a hard failure instead of silently skipping those cases. CI uses this for the dedicated Postgres lanes.

The `qm-api` integration tests live under `crates/qm-api/tests/` and are organized by what they cover, not by implementation milestone. Keep new test files behavior-oriented too: for example `invites.rs`, `households.rs`, and `stock_lifecycle.rs`, not `phase7.rs` or `*_slice.rs`.

## Container image

The repository ships a generic `Dockerfile` for self-hosting platforms that can run OCI images.

Build the image:

```sh
docker build -t quartermaster:latest .
```

Run it directly:

```sh
docker run --rm \
  -p 8080:8080 \
  -e QM_DATABASE_URL=sqlite:///data/data.db?mode=rwc \
  -v quartermaster-data:/data \
  quartermaster:latest
```

The image contract is intentionally small:

- configuration is done through `QM_*` environment variables
- the app listens on port `8080`
- `/data` is the recommended writable mount point for SQLite
- `docker compose` is optional convenience, not the deployment model

An example `compose.yaml` is included for local or small-server setups:

```sh
docker compose up --build
```

If you deploy on another platform such as Fly.io, Nomad, Kubernetes, or systemd+Podman, use the same image and environment-variable contract rather than treating Compose as special.

## Regenerating the OpenAPI spec

```sh
cargo xtask export-openapi
```

Writes `openapi.json` at the repo root **and** at `ios/Quartermaster/openapi.json`. The iOS target's Xcode build-tool plugin reads the second copy; the first is the canonical one (external consumers, CI drift check). Commit both so the iOS build stays hermetic.

Re-run this after any change to a Rust DTO, route, or enum — the next iOS build will regenerate `Components.Schemas.*` and the generated `Client` automatically.

Invite-backed registration and `POST /invites/redeem` are transactional: creating the user/membership and consuming the invite happen together, and redeeming an invite for a household the user already belongs to is treated as an idempotent no-op rather than consuming another use.

## Universal Links

HTTPS invite links are built from `QM_PUBLIC_BASE_URL` when it is set. For direct app-opening on iOS, that public HTTPS origin must also serve `/.well-known/apple-app-site-association`, and the app build must include a matching `applinks:` associated domain. Quartermaster keeps `/join` as the browser fallback so shared links still work when the app is not installed.

## Contributing

The v1 scope is intentionally narrow — see the status section. Please open an issue to discuss feature ideas before writing code.

## Open Food Facts & ODbL

Barcode lookups hit the [Open Food Facts](https://world.openfoodfacts.org) public API, and the server caches the result locally in `barcode_cache`. Open Food Facts data is licensed under the [Open Database Licence (ODbL) v1.0](https://opendatacommons.org/licenses/odbl/1-0/). That has implications for anyone *redistributing* a Quartermaster instance's database (self-hosters who only use it privately are fine). A more thorough audit is tracked in [TODO.md](TODO.md) under cross-cutting work.

## License

Apache License 2.0 — see [LICENSE](LICENSE).
