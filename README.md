# Quartermaster

Quartermaster is a self-hosted kitchen inventory app for households.

It helps you keep track of what is in your pantry, fridge, and freezer; what is about to expire; and what needs to be used up. You can add stock manually, search existing products, scan barcodes on supported mobile devices, group stock by location, review inventory history, and receive expiry reminders.

There is no hosted Quartermaster service today. To use it, you run your own Quartermaster server and connect the mobile or web clients to it.

## Status

Quartermaster is usable today for adventurous self-hosters and is still evolving quickly. Expect occasional breaking changes between releases and read [CHANGELOG.md](CHANGELOG.md) before upgrading a running household.

- **Server:** Rust API, SQLite by default, optional Postgres, Docker image support, local accounts, invite-based household sharing, barcode lookup, stock history, reminders, push-worker support, and optional Prometheus metrics.
- **iOS:** Native SwiftUI client is the primary client. It supports onboarding, sign-in, household switching, inventory, stock creation/editing/consumption, barcode scanning on physical devices, history, settings, invite links, and reminders.
- **Android:** Native Jetpack Compose client exists and can connect to self-hosted servers. It supports the core account, inventory, reminder, and invite flows, with push configuration available for self-hosters who provide Firebase details.
- **Web:** A SvelteKit web client is included and can be served by the API process. It supports core inventory, location, product, barcode, history, invite, settings, and reminder flows. The native mobile apps remain the most complete experience.

Quartermaster is intentionally narrow: it is inventory software, not recipe planning, grocery automation, or a general household task app.

## What You Need

- A machine that can run an OCI container, or a Rust toolchain if you want to run from source.
- A persistent database location. SQLite is the simplest option and stores everything in one file.
- A reverse proxy with HTTPS if you want to use it outside your home network.
- Optional: APNs and/or Firebase Cloud Messaging credentials if you want push notifications instead of only in-app reminder inboxes.

## Quick Start With Docker

Build the image:

```sh
docker build -t quartermaster:latest .
```

Run it with a persistent SQLite volume:

```sh
docker run --rm \
  -p 8080:8080 \
  -e QM_DATABASE_URL=sqlite:///data/data.db?mode=rwc \
  -v quartermaster-data:/data \
  quartermaster:latest
```

Then open:

- `http://localhost:8080/healthz` for a health check
- `http://localhost:8080/docs` for the API explorer
- `http://localhost:8080/` for the built web shell, when present in the image

An example Compose file is included for small local setups:

```sh
docker compose up --build
```

Compose is only convenience. The deployment contract is the same everywhere: run the image, expose port `8080`, set `QM_*` environment variables, and mount persistent storage for SQLite if you use it.

## Running From Source

```sh
cargo run -p qm-server
```

By default the server listens on `0.0.0.0:8080` and creates `data.db` in the current directory.

## First Setup

The default registration mode is `first_run_only`. That means the first person can create an account and household, then further users should join by invite.

1. Start the server.
2. Open the iOS app, Android app, or web shell.
3. Enter your server URL.
4. Create the first account and household.
5. Create invite codes from Settings for other household members.

Users can belong to multiple households. Each signed-in session keeps its own selected household, and switching households only affects that current session.

## Configuration

Common settings:

| Variable               | Default                     | Meaning                                             |
| ---------------------- | --------------------------- | --------------------------------------------------- |
| `QM_BIND`              | `0.0.0.0:8080`              | Server bind address                                 |
| `QM_DATABASE_URL`      | `sqlite://data.db?mode=rwc` | SQLite or Postgres connection string                |
| `QM_LOG_FORMAT`        | `text`                      | `text` or `json` logs                               |
| `QM_REGISTRATION_MODE` | `first_run_only`            | `first_run_only`, `invite_only`, or `open`          |
| `QM_PUBLIC_BASE_URL`   | unset                       | Public HTTPS origin for invite/share links          |
| `QM_WEB_DIST_DIR`      | `web/build`                 | Built web shell directory served by the API process |
| `QM_WEB_AUTH_ALLOWED_ORIGINS` | unset                | Comma-separated HTTPS browser origins allowed to use credentialed cookie auth |
| `RUST_LOG`             | `info`                      | Tracing filter                                      |

Rate limiting and reverse proxies:

| Variable                            | Default  | Meaning                                                                    |
| ----------------------------------- | -------- | -------------------------------------------------------------------------- |
| `QM_RATE_LIMIT_CLIENT_IP_MODE`      | `socket` | Use `socket` directly, or `x-forwarded-for` behind a trusted reverse proxy |
| `QM_RATE_LIMIT_TRUSTED_PROXY_CIDRS` | unset    | Comma-separated trusted proxy CIDRs allowed to supply `X-Forwarded-For`    |
| `QM_RATE_LIMIT_AUTH_PER_MINUTE`     | `10`     | Per-client auth refill rate                                                |
| `QM_RATE_LIMIT_AUTH_BURST`          | `5`      | Per-client auth burst                                                      |
| `QM_RATE_LIMIT_BARCODE_PER_MINUTE`  | `60`     | Per-client barcode lookup refill rate                                      |
| `QM_RATE_LIMIT_BARCODE_BURST`       | `20`     | Per-client barcode lookup burst                                            |
| `QM_RATE_LIMIT_HISTORY_PER_MINUTE`  | `120`    | Per-client history refill rate                                             |
| `QM_RATE_LIMIT_HISTORY_BURST`       | `40`     | Per-client history burst                                                   |

Barcode lookup tuning:

| Variable                                   | Default                                          | Meaning                                              |
| ------------------------------------------ | ------------------------------------------------ | ---------------------------------------------------- |
| `QM_OFF_API_BASE_URL`                      | `https://world.openfoodfacts.org/api/v2/product` | Barcode product API base URL                         |
| `QM_OFF_TIMEOUT_SECONDS`                   | `5`                                              | Timeout for one barcode lookup request               |
| `QM_OFF_MAX_RETRIES`                       | `2`                                              | Retry count for transient lookup failures            |
| `QM_OFF_RETRY_BASE_DELAY_MS`               | `200`                                            | Base retry backoff                                   |
| `QM_OFF_CIRCUIT_BREAKER_FAILURE_THRESHOLD` | `5`                                              | Consecutive transient failures before fail-fast mode |
| `QM_OFF_CIRCUIT_BREAKER_OPEN_SECONDS`      | `60`                                             | Fail-fast duration                                   |

Reminder settings:

| Variable                                    | Default | Meaning                                                      |
| ------------------------------------------- | ------- | ------------------------------------------------------------ |
| `QM_EXPIRY_REMINDERS_ENABLED`               | `false` | Enables backend expiry reminder generation                   |
| `QM_EXPIRY_REMINDER_LEAD_DAYS`              | `1`     | Days before expiry when reminders fire                       |
| `QM_EXPIRY_REMINDER_FIRE_HOUR`              | `9`     | Household-local reminder hour                                |
| `QM_EXPIRY_REMINDER_FIRE_MINUTE`            | `0`     | Household-local reminder minute                              |
| `QM_EXPIRY_REMINDER_SWEEP_INTERVAL_SECONDS` | `0`     | In-process reminder reconciliation interval; `0` disables it |
| `QM_EXPIRY_REMINDER_TRIGGER_SECRET`         | unset   | Enables the internal manual reminder sweep endpoint          |

Household expiry dates are calendar dates in the household timezone. Reminder fire times are computed in that same household-local timezone and stored as UTC instants.

## Push Reminders

Quartermaster has a durable reminder inbox even without push notifications. Clients can poll due reminders and users explicitly acknowledge them.

Push delivery is optional. It can run in the main API process:

```sh
QM_PUSH_WORKER_ENABLED=true cargo run -p qm-server
```

Or as a separate worker process:

```sh
cargo run -p qm-server -- push-worker
```

Push-related settings:

| Variable                               | Default   | Meaning                                    |
| -------------------------------------- | --------- | ------------------------------------------ |
| `QM_PUSH_WORKER_ENABLED`               | `false`   | Run the push worker inside the API process |
| `QM_PUSH_WORKER_POLL_INTERVAL_SECONDS` | `30`      | Worker polling interval                    |
| `QM_PUSH_WORKER_BATCH_SIZE`            | `25`      | Max deliveries claimed per cycle           |
| `QM_PUSH_WORKER_CLAIM_TTL_SECONDS`     | `60`      | Claim timeout before retry                 |
| `QM_PUSH_WORKER_RETRY_BACKOFF_SECONDS` | `300`     | Retry delay after retryable failures       |
| `QM_APNS_ENABLED`                      | `false`   | Enable iOS APNs delivery                                |
| `QM_APNS_ENVIRONMENT`                  | `sandbox` | `sandbox` or `production`                               |
| `QM_APNS_TOPIC`                        | unset     | APNs topic / bundle identifier                          |
| `QM_APNS_AUTH_TOKEN`                   | unset     | APNs bearer token; takes precedence over JWT config     |
| `QM_APNS_KEY_ID`                       | unset     | APNs signing key ID for `.p8` JWT auth                  |
| `QM_APNS_TEAM_ID`                      | unset     | Apple developer team ID for `.p8` JWT auth              |
| `QM_APNS_PRIVATE_KEY_PATH`             | unset     | APNs `.p8` private-key path                             |
| `QM_APNS_PRIVATE_KEY`                  | unset     | APNs `.p8` private-key content                          |
| `QM_FCM_ENABLED`                       | `false`   | Enable Android FCM delivery                             |
| `QM_FCM_PROJECT_ID`                    | unset     | Firebase project ID                                     |
| `QM_FCM_SERVICE_ACCOUNT_JSON_PATH`     | unset     | Firebase service-account JSON path                      |
| `QM_FCM_SERVICE_ACCOUNT_JSON`          | unset     | Firebase service-account JSON content; mutually exclusive with path |

For a fuller reminder deployment walkthrough, see [docs/hosted-reminders.md](docs/hosted-reminders.md).

## Metrics And Maintenance

Quartermaster exposes a small set of internal maintenance hooks when you configure shared secrets. These routes are not part of the public API contract.

| Variable                                 | Default          | Meaning                                                |
| ---------------------------------------- | ---------------- | ------------------------------------------------------ |
| `QM_AUTH_SESSION_SWEEP_INTERVAL_SECONDS` | `0`              | Periodic stale-session sweep interval; `0` disables it |
| `QM_AUTH_SESSION_SWEEP_TRIGGER_SECRET`   | unset            | Enables manual auth-session sweeping                   |
| `QM_METRICS_ENABLED`                     | `false`          | Enables internal Prometheus metrics                    |
| `QM_METRICS_BIND`                        | `127.0.0.1:9091` | Dedicated metrics/health bind for split worker mode    |
| `QM_METRICS_TRIGGER_SECRET`              | unset            | Required token for `GET /internal/metrics`             |

When metrics are enabled, callers must supply `X-QM-Maintenance-Token`.

## Invite Links And iOS Universal Links

If `QM_PUBLIC_BASE_URL` is set, it must be an `https://` origin with no path, query, or fragment. Quartermaster uses it for browser-friendly invite links.

For iOS Universal Links, configure:

| Variable           | Meaning                                        |
| ------------------ | ---------------------------------------------- |
| `QM_IOS_TEAM_ID`   | Apple Team ID used in the AASA payload         |
| `QM_IOS_BUNDLE_ID` | iOS bundle identifier used in the AASA payload |

The iOS app build also needs a matching associated-domain entitlement. Without that setup, invite links still work through the browser fallback and manual invite entry.

## Clients

The native clients are not distributed through public app stores yet. For now, self-hosters build them from this repository:

- iOS setup lives in [ios/README.md](ios/README.md).
- Android setup lives in [android/README.md](android/README.md).
- The web shell is built into the container image and can also be built from `web/`.

## Contributing

Development setup, test commands, API generation, and repository conventions live in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache License 2.0. See [LICENSE](LICENSE).
