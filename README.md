# Quartermaster

A mobile-first, self-hostable kitchen inventory management system.

Quartermaster tells you what's in your kitchen, what's about to expire, and lets you add stock quickly by scanning a barcode. That's it. No recipes, no meal planning, no to-do lists — at least not for now. It's a leaner, inventory-focused alternative to [Grocy](https://grocy.info).

## Status

Early work in progress. v1 is being built toward an "empty pantry" first vertical slice.

## Architecture

- **Backend:** Rust (Axum + SQLx + Tokio), single self-hosted binary
- **Database:** SQLite by default (one `.db` file), Postgres optional via config
- **Mobile:** native clients. iOS first (SwiftUI, iOS 26, Liquid Glass), then Android, then web. iOS types + HTTP client are generated at Xcode build time from the checked-in OpenAPI spec via [swift-openapi-generator](https://github.com/apple/swift-openapi-generator); the Android app generates its Retrofit client from that same checked-in spec during the Gradle build.
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
├── android/                Jetpack Compose app + generated Retrofit client
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
| `QM_PUBLIC_BASE_URL`    | unset                       | Public HTTPS origin used in invite/share links |
| `QM_IOS_TEAM_ID`        | unset                       | Optional Apple Team ID for serving the AASA payload; required only when publishing universal-link identity |
| `QM_IOS_BUNDLE_ID`      | unset                       | Optional iOS bundle identifier for serving the AASA payload; required only when publishing universal-link identity |
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
| `QM_RATE_LIMIT_TRUSTED_PROXY_CIDRS`       | unset                                             | Comma-separated CIDRs whose socket IPs are allowed to supply `X-Forwarded-For` |
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
| `QM_AUTH_SESSION_SWEEP_INTERVAL_SECONDS`   | `0`                                               | Periodic stale-session sweep interval in seconds; `0` disables the in-process timer |
| `QM_AUTH_SESSION_SWEEP_TRIGGER_SECRET`     | unset                                             | Enables `POST /internal/maintenance/sweep-auth-sessions` when set; callers must supply the shared secret in `X-QM-Maintenance-Token` |
| `QM_EXPIRY_REMINDERS_ENABLED`              | `false`                                           | Enables backend-owned expiry reminder generation |
| `QM_EXPIRY_REMINDER_LEAD_DAYS`             | `1`                                               | How many days before expiry a reminder should fire |
| `QM_EXPIRY_REMINDER_FIRE_HOUR`             | `9`                                               | Household-local hour when expiry reminders should fire |
| `QM_EXPIRY_REMINDER_FIRE_MINUTE`           | `0`                                               | Household-local minute when expiry reminders should fire |
| `QM_EXPIRY_REMINDER_SWEEP_INTERVAL_SECONDS`| `0`                                               | Periodic reminder reconciliation interval in seconds; `0` disables the in-process timer |
| `QM_EXPIRY_REMINDER_TRIGGER_SECRET`        | unset                                             | Enables `POST /internal/maintenance/sweep-expiry-reminders` when set; callers must supply the shared secret in `X-QM-Maintenance-Token` |
| `QM_PUSH_WORKER_ENABLED`                   | `false`                                           | Runs the APNs delivery worker inside the main API process when `true` and APNs is configured |
| `QM_PUSH_WORKER_POLL_INTERVAL_SECONDS`     | `30`                                              | How often the worker scans for due reminder deliveries |
| `QM_PUSH_WORKER_BATCH_SIZE`                | `25`                                              | Maximum reminder/device deliveries to claim in one cycle |
| `QM_PUSH_WORKER_CLAIM_TTL_SECONDS`         | `60`                                              | How long a claimed delivery stays reserved before it is considered stale |
| `QM_PUSH_WORKER_RETRY_BACKOFF_SECONDS`     | `300`                                             | Retry delay after a retryable push failure or expired claim |
| `QM_APNS_ENABLED`                          | `false`                                           | Enables APNs delivery support |
| `QM_APNS_ENVIRONMENT`                      | `sandbox`                                         | APNs environment: `sandbox` or `production` |
| `QM_APNS_TOPIC`                            | unset                                             | APNs topic / bundle identifier used for reminder notifications |
| `QM_APNS_AUTH_TOKEN`                       | unset                                             | APNs bearer token; must be supplied when APNs is enabled |
| `QM_APNS_BASE_URL`                         | unset                                             | Optional APNs base URL override for local development or testing |
| `QM_METRICS_ENABLED`                       | `false`                                           | Enables internal Prometheus metrics exposure |
| `QM_METRICS_BIND`                          | `127.0.0.1:9091`                                  | Bind address for the dedicated `push-worker` metrics/health server |
| `QM_METRICS_TRIGGER_SECRET`                | unset                                             | Required when metrics are enabled; callers must supply it in `X-QM-Maintenance-Token` for `GET /internal/metrics` |

When `QM_PUBLIC_BASE_URL` is set, Quartermaster validates it strictly at startup: it must be an `https://` origin with no path, query, or fragment. The server normalizes a trailing slash away before exposing it to clients.

Keep `QM_RATE_LIMIT_CLIENT_IP_MODE=socket` for direct deployments or simple local setups. Switch to `x-forwarded-for` only when Quartermaster sits behind a trusted reverse proxy that overwrites `X-Forwarded-For`, and set `QM_RATE_LIMIT_TRUSTED_PROXY_CIDRS` to the proxy subnet(s). Quartermaster ignores `X-Forwarded-For` unless the connecting peer IP matches one of those trusted CIDRs.

Stale `auth_session` rows are still cleaned up opportunistically during auth flows. For long-lived deployments you can also opt into a periodic in-process sweep with `QM_AUTH_SESSION_SWEEP_INTERVAL_SECONDS`, or keep the timer disabled and trigger `POST /internal/maintenance/sweep-auth-sessions` from external automation. That maintenance route is intentionally not part of the public OpenAPI surface.

Expiry reminders are also backend-owned in the current v1 design: the server computes reminder timing and wording once, stores pending reminder rows, and clients poll `GET /reminders` rather than reimplementing the policy locally. `QM_EXPIRY_REMINDER_FIRE_HOUR` and `QM_EXPIRY_REMINDER_FIRE_MINUTE` are interpreted in each household's configured timezone, then stored canonically in UTC.

## Reminder Delivery

Quartermaster's reminder delivery pipeline has two pieces:

- the main API process, which owns stock mutations, reminder reconciliation, and the reminder inbox API
- the APNs worker, which claims due reminder/device deliveries, sends push requests, and writes durable delivery attempts

You can run the worker in either deployment shape:

- integrated mode: run `cargo run -p qm-server` with `QM_PUSH_WORKER_ENABLED=true`
- split mode: run the API normally and start a second process with `cargo run -p qm-server -- push-worker`

`push-worker` mode requires `QM_APNS_ENABLED=true` and `QM_APNS_TOPIC` to be configured. In split deployments, the sweeper remains a repair tool for drift; the worker is still the primary delivery path.

For a hosted deployment walkthrough, maintenance examples, and an operator troubleshooting checklist, see [docs/hosted-reminders.md](docs/hosted-reminders.md).

### Metrics and Health

When `QM_METRICS_ENABLED=true`, Quartermaster exposes an internal Prometheus scrape surface protected by `X-QM-Maintenance-Token`:

- API process: `GET /internal/metrics` on the normal API bind address
- dedicated worker: `GET /internal/metrics` and `GET /healthz` on `QM_METRICS_BIND`

These routes are intentionally internal-only and are not part of the public OpenAPI contract.

The worker publishes counters and gauges for:

- cycle count and cycle failures
- claimed deliveries, claim conflicts, and expired claims
- push attempts by outcome (`succeeded`, `failed_retryable`, `failed_permanent`)
- transport failures before any APNs response
- send/cycle latency histograms
- due reminder count, retry-due count, active claim count, permanent/retryable failure counts, invalid-token count, and oldest due reminder age

Healthy delivery usually looks like:

- `qm_push_worker_last_cycle_completed_timestamp_seconds` continuing to advance
- `qm_reminders_oldest_due_age_seconds` staying low
- `qm_push_deliveries_active_claim_count` briefly rising during sends and then draining
- `qm_push_devices_with_invalid_token_count` staying near zero

Potential trouble signs are:

- oldest-due age growing steadily
- retry-due count or active-claim count staying elevated for long periods
- frequent claim conflicts across multiple workers
- permanent token failures accumulating faster than devices refresh their push tokens

An APNs permanent token failure means the server has stopped retrying that reminder for that exact device token. Delivery resumes automatically if the client later re-registers the device with a new token.

Example Prometheus scrape config:

```yaml
scrape_configs:
  - job_name: quartermaster-api
    metrics_path: /internal/metrics
    static_configs:
      - targets: ["quartermaster.example.com:8080"]
    headers:
      X-QM-Maintenance-Token: "${QM_METRICS_TRIGGER_SECRET}"

  - job_name: quartermaster-push-worker
    metrics_path: /internal/metrics
    static_configs:
      - targets: ["127.0.0.1:9091"]
    headers:
      X-QM-Maintenance-Token: "${QM_METRICS_TRIGGER_SECRET}"
```

For small hosted deployments, the default worker timings are intentionally conservative:

- `QM_PUSH_WORKER_POLL_INTERVAL_SECONDS=30`
- `QM_PUSH_WORKER_CLAIM_TTL_SECONDS=60`
- `QM_PUSH_WORKER_RETRY_BACKOFF_SECONDS=300`

Those defaults are a good starting point unless backlog age or retry volume shows they need tuning.

### Production-Shaped Reminder Setup

For small hosted deployments, the recommended v1 shape is:

- API process with `QM_METRICS_ENABLED=true`
- separate `cargo run -p qm-server -- push-worker` process
- APNs configured on the worker
- maintenance triggers enabled for auth-session and reminder sweeps

Example maintenance calls:

```sh
curl -X POST \
  -H "X-QM-Maintenance-Token: $QM_AUTH_SESSION_SWEEP_TRIGGER_SECRET" \
  https://quartermaster.example.com/internal/maintenance/sweep-auth-sessions

curl -X POST \
  -H "X-QM-Maintenance-Token: $QM_EXPIRY_REMINDER_TRIGGER_SECRET" \
  https://quartermaster.example.com/internal/maintenance/sweep-expiry-reminders
```

The sweeper endpoints repair drift. They do not replace the running push worker.

## Tests

`cargo test --workspace` is the default fast verification pass.

`cargo xtask verify-release-config` checks that the env-driven backend AASA identity matches the env-driven iOS release identity and associated-domain host.

Use `cargo xtask verify-release-config` after any change to:

- `QM_PUBLIC_BASE_URL`
- the backend Apple App Site Association payload
- `ios/project.yml` team ID, bundle ID, or associated-domain identity wiring

If you changed reminder scheduling, reminder delivery, or hosted push-worker wiring, validate with `cargo test --workspace` and then do one split-worker smoke test locally (`cargo run -p qm-server` plus `cargo run -p qm-server -- push-worker`).

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

Release builds of the iOS app fail if `DEVELOPMENT_TEAM`, `PRODUCT_BUNDLE_IDENTIFIER`, or `QUARTERMASTER_ASSOCIATED_DOMAIN` are missing or malformed. Local development can keep using the custom `quartermaster://` scheme without setting `QM_PUBLIC_BASE_URL`, and the backend only serves the AASA payload when `QM_IOS_TEAM_ID` plus `QM_IOS_BUNDLE_ID` are configured.

Quartermaster supports one explicit v1 release identity story:

- one production `QM_PUBLIC_BASE_URL`
- one associated-domain host
- one env-driven iOS team ID + bundle ID pairing

Keep those aligned and use `cargo xtask verify-release-config` as the drift check rather than checking Apple release identity into the repo.

## Contributing

The v1 scope is intentionally narrow — see the status section. Please open an issue to discuss feature ideas before writing code.

## Open Food Facts & ODbL

Barcode lookups hit the [Open Food Facts](https://world.openfoodfacts.org) public API, and the server caches the result locally in `barcode_cache`. That table stores the looked-up barcode, the linked Quartermaster product ID when one exists, the raw OFF JSON payload for successful lookups, the fetch timestamp, and a miss flag for negative cache entries.

Open Food Facts data is licensed under the [Open Database Licence (ODbL) v1.0](https://opendatacommons.org/licenses/odbl/1-0/). For most private self-hosting, that is low risk: if you are only using Quartermaster for your own household and not redistributing the database or derived exports, the usual ODbL sharing triggers are unlikely to apply.

The obligations matter more if you redistribute a Quartermaster database, publish hosted exports built from cached OFF data, or ship backups/dumps outside your private household use. In those cases, review OFF attribution and ODbL share-alike requirements before distributing the data. Quartermaster does not currently automate that compliance work for operators.

## Self-Hosting Note

If you plan to publish database snapshots, host a shared/public Quartermaster service, or export cached barcode data outside your private deployment, review the Open Food Facts / ODbL obligations first. The cached OFF payloads live in your application database, so backup and export workflows can become the point where redistribution rules matter.

## License

Apache License 2.0 — see [LICENSE](LICENSE).
