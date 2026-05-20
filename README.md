# Quartermaster

Quartermaster is a self-hosted kitchen inventory app for households.

It helps you keep track of what is in your pantry, fridge, and freezer; what is about to expire; and what needs to be used up. You can add stock manually, search existing products, scan barcodes on supported mobile devices, group stock by location, review inventory history, print QR stock labels, and receive expiry reminders.

There is no hosted Quartermaster service today. To use it, you run your own Quartermaster server and connect the mobile or web clients to it.

## Status

Quartermaster is usable today for adventurous self-hosters and is still evolving quickly. Expect occasional breaking changes between releases and read [CHANGELOG.md](CHANGELOG.md) before upgrading a running household.

- **Server:** Rust API, SQLite by default, optional Postgres, Docker image support, local accounts, browser cookie auth, optional passkeys, verified recovery-email state, invite-based household sharing, authenticated mobile handoff, OpenFoodFacts barcode lookup/cache/contribution, stock history, reminders, background worker support, Brother QL label printing, and optional Prometheus metrics.
- **iOS:** Native SwiftUI client is the primary client. It supports onboarding, sign-in, passkeys, authenticated mobile handoff, household switching, inventory, stock creation/editing/consumption, product editing, OpenFoodFacts correction contribution, barcode scanning on physical devices, history, recovery-email setup, settings, invite links, reminder push/inbox flows, and printing stock labels through configured server-side printers.
- **Android:** Native Jetpack Compose client exists and can connect to self-hosted servers. It supports the core account, passkey, authenticated handoff, inventory, product, location, reminder, recovery-email, barcode, OpenFoodFacts credential/contribution, scan/add-stock, and invite flows, with push configuration available for self-hosters who provide Firebase details.
- **Web:** A SvelteKit web client is included and can be served by the API process. It supports core inventory, batch deep links, split/repack workflows, location, product, OpenFoodFacts credential/contribution, barcode, history, invite, settings, label-printer administration, mobile setup QR, and reminder flows. The native mobile apps remain the most complete experience.

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
- `http://localhost:8080/api/v1/openapi.json` for the generated API spec
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

By default the server listens on `[::]:8080` with dual-stack IPv4/IPv6 support and creates `data.db` in the current directory.

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

| Variable                       | Default                     | Meaning                                                                       |
| ------------------------------ | --------------------------- | ----------------------------------------------------------------------------- |
| `QM_BIND`                      | `[::]:8080`                 | Server bind address                                                           |
| `QM_DATABASE_URL`              | `sqlite://data.db?mode=rwc` | SQLite or Postgres connection string                                          |
| `QM_LOG_FORMAT`                | `text`                      | `text` or `json` logs                                                         |
| `QM_REGISTRATION_MODE`         | `first_run_only`            | `first_run_only`, `invite_only`, or `open`                                    |
| `QM_ACCESS_TOKEN_TTL_SECONDS`  | `1800`                      | Access-token lifetime in seconds                                              |
| `QM_REFRESH_TOKEN_TTL_SECONDS` | `5184000`                   | Refresh-token lifetime in seconds                                             |
| `QM_INVITE_TTL_SECONDS`        | `604800`                    | Invite-code lifetime in seconds                                               |
| `QM_PUBLIC_BASE_URL`           | unset                       | Public HTTP(S) origin for invite/share links and app setup                    |
| `QM_PASSKEYS_ENABLED`          | `false`                     | Enables WebAuthn/passkey registration and sign-in when RP config is complete  |
| `QM_PASSKEY_RP_ID`             | derived                     | Optional WebAuthn relying-party ID; defaults to `QM_PUBLIC_BASE_URL` host     |
| `QM_PASSKEY_ORIGIN`            | derived                     | Optional WebAuthn origin; defaults to `QM_PUBLIC_BASE_URL`                    |
| `QM_PASSKEY_RP_NAME`           | `Quartermaster`             | Human-readable relying-party name shown by passkey platforms                  |
| `QM_WEB_DIST_DIR`              | `web/build`                 | Built web shell directory served by the API process                           |
| `QM_WEB_AUTH_ALLOWED_ORIGINS`  | unset                       | Comma-separated HTTPS browser origins allowed to use credentialed cookie auth |
| `RUST_LOG`                     | `info`                      | Tracing filter                                                                |

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

Keep `QM_RATE_LIMIT_CLIENT_IP_MODE=socket` when Quartermaster receives traffic directly from clients. Behind a trusted reverse proxy, set `QM_RATE_LIMIT_CLIENT_IP_MODE=x-forwarded-for` and set `QM_RATE_LIMIT_TRUSTED_PROXY_CIDRS` to the proxy source ranges that are allowed to supply `X-Forwarded-For`. Do not trust all private ranges unless every service in those ranges is allowed to choose client IPs for rate limiting.

If the web shell is served from a different HTTPS origin than the API, list that browser origin in `QM_WEB_AUTH_ALLOWED_ORIGINS` so cookie-backed browser auth can use credentialed CORS. Entries must be origins only, with no path:

```sh
QM_PUBLIC_BASE_URL=https://api.quartermaster.example.com
QM_WEB_AUTH_ALLOWED_ORIGINS=https://app.quartermaster.example.com
```

A reverse proxy for that split-origin shape should forward the API host to Quartermaster and preserve the client address only from trusted proxy hops. For example, in Caddy:

```caddyfile
api.quartermaster.example.com {
	reverse_proxy quartermaster:8080
}

app.quartermaster.example.com {
	root * /srv/quartermaster-web
	file_server
}
```

And in nginx:

```nginx
server {
    server_name api.quartermaster.example.com;

    location / {
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_pass http://quartermaster:8080;
    }
}
```

Barcode lookup tuning:

| Variable                                   | Default                                               | Meaning                                               |
| ------------------------------------------ | ----------------------------------------------------- | ----------------------------------------------------- |
| `QM_OFF_API_BASE_URL`                      | `https://world.openfoodfacts.org/api/v2/product`      | Barcode product API base URL                          |
| `QM_OFF_WRITE_URL`                         | `https://world.openfoodfacts.org/cgi/product_jqm2.pl` | OpenFoodFacts product contribution endpoint           |
| `QM_OFF_CREDENTIAL_ENCRYPTION_KEY`         | unset                                                 | Secret used to encrypt per-user OFF credentials       |
| `QM_OFF_POSITIVE_TTL_DAYS`                 | `30`                                                  | Freshness window for successful barcode cache entries |
| `QM_OFF_NEGATIVE_TTL_DAYS`                 | `7`                                                   | Freshness window for barcode miss cache entries       |
| `QM_OFF_TIMEOUT_SECONDS`                   | `5`                                                   | Timeout for one barcode lookup request                |
| `QM_OFF_MAX_RETRIES`                       | `2`                                                   | Retry count for transient lookup failures             |
| `QM_OFF_RETRY_BASE_DELAY_MS`               | `200`                                                 | Base retry backoff                                    |
| `QM_OFF_CIRCUIT_BREAKER_FAILURE_THRESHOLD` | `5`                                                   | Consecutive transient failures before fail-fast mode  |
| `QM_OFF_CIRCUIT_BREAKER_OPEN_SECONDS`      | `60`                                                  | Fail-fast duration                                    |

Reminder settings:

| Variable                                    | Default | Meaning                                                      |
| ------------------------------------------- | ------- | ------------------------------------------------------------ |
| `QM_EXPIRY_REMINDERS_ENABLED`               | `false` | Enables backend expiry reminder generation                   |
| `QM_EXPIRY_REMINDER_LEAD_DAYS`              | `1`     | Days before expiry when reminders fire                       |
| `QM_EXPIRY_REMINDER_FIRE_HOUR`              | `9`     | Household-local reminder hour                                |
| `QM_EXPIRY_REMINDER_FIRE_MINUTE`            | `0`     | Household-local reminder minute                              |
| `QM_EXPIRY_REMINDER_RECONCILE_INTERVAL_SECONDS` | `0`     | Interval for API pods to enqueue household reminder reconcile jobs |
| `QM_EXPIRY_REMINDER_TRIGGER_SECRET`             | unset   | Enables the internal manual reminder job enqueue endpoint          |

Household expiry dates are calendar dates in the household timezone. Reminder fire times are computed in that same household-local timezone and stored as UTC instants.

## Push Reminders

Quartermaster has a durable reminder inbox even without push notifications. Clients can poll due reminders and users explicitly acknowledge them.

Push delivery is optional. Run it from the background worker process:

```sh
cargo run -p qm-server -- worker
```

Worker and push-related settings:

| Variable                               | Default   | Meaning                                                             |
| -------------------------------------- | --------- | ------------------------------------------------------------------- |
| `QM_WORKER_POLL_INTERVAL_SECONDS`      | `30`      | Worker polling interval                                             |
| `QM_WORKER_BATCH_SIZE`                 | `25`      | Max jobs or push deliveries claimed per cycle                       |
| `QM_WORKER_LEASE_TTL_SECONDS`          | `60`      | Claim timeout before retry                                          |
| `QM_WORKER_RETRY_BACKOFF_SECONDS`      | `300`     | Retry delay after retryable failures                                |
| `QM_WORKER_ID`                         | generated | Optional stable worker identity for leases                          |
| `QM_APNS_ENABLED`                      | `false`   | Enable iOS APNs delivery                                            |
| `QM_APNS_ENVIRONMENT`                  | `sandbox` | `sandbox` or `production`                                           |
| `QM_APNS_TOPIC`                        | unset     | APNs topic / bundle identifier                                      |
| `QM_APNS_AUTH_TOKEN`                   | unset     | APNs bearer token; takes precedence over JWT config                 |
| `QM_APNS_KEY_ID`                       | unset     | APNs signing key ID for `.p8` JWT auth                              |
| `QM_APNS_TEAM_ID`                      | unset     | Apple developer team ID for `.p8` JWT auth                          |
| `QM_APNS_PRIVATE_KEY_PATH`             | unset     | APNs `.p8` private-key path                                         |
| `QM_APNS_PRIVATE_KEY`                  | unset     | APNs `.p8` private-key content                                      |
| `QM_APNS_BASE_URL`                     | unset     | APNs endpoint override for local/provider testing                   |
| `QM_FCM_ENABLED`                       | `false`   | Enable Android FCM delivery                                         |
| `QM_FCM_PROJECT_ID`                    | unset     | Firebase project ID                                                 |
| `QM_FCM_SERVICE_ACCOUNT_JSON_PATH`     | unset     | Firebase service-account JSON path                                  |
| `QM_FCM_SERVICE_ACCOUNT_JSON`          | unset     | Firebase service-account JSON content; mutually exclusive with path |
| `QM_FCM_BASE_URL`                      | unset     | FCM send endpoint override for local/provider testing               |
| `QM_FCM_TOKEN_URL`                     | unset     | OAuth token endpoint override for local/provider testing            |

For a fuller reminder deployment walkthrough, see [docs/hosted-reminders.md](docs/hosted-reminders.md).

## Label Printers

Quartermaster can print QR stock labels through server-side Brother QL raster printers reachable from the API process. The web Settings screen manages household printer definitions, and stock batch surfaces can send labels once a default printer is configured.

Label QR codes point at `/batches/{id}` under `QM_PUBLIC_BASE_URL`, so set that public origin before printing labels. The current printer driver is `brother_ql_raster` over TCP port `9100`, with DK 62 mm continuous, DK 62 mm red/black continuous, and DK 29x90 media options.

## Metrics And Maintenance

Quartermaster exposes a small set of internal maintenance hooks when you configure shared secrets. These routes are not part of the public API contract.

| Variable                                 | Default          | Meaning                                                |
| ---------------------------------------- | ---------------- | ------------------------------------------------------ |
| `QM_AUTH_SESSION_CLEANUP_INTERVAL_SECONDS` | `0`              | Interval for API pods to enqueue stale-session cleanup jobs |
| `QM_AUTH_SESSION_SWEEP_TRIGGER_SECRET`     | unset            | Enables manual auth-session cleanup job enqueueing          |
| `QM_SMOKE_SEED_TRIGGER_SECRET`           | unset            | Enables the internal local smoke-fixture seed route    |
| `QM_METRICS_ENABLED`                     | `false`          | Enables internal Prometheus metrics                    |
| `QM_METRICS_BIND`                        | `127.0.0.1:9091` | Dedicated metrics/health bind for split worker mode    |
| `QM_METRICS_TRIGGER_SECRET`              | unset            | Required token for `GET /internal/metrics`             |

When metrics are enabled, callers must supply `X-QM-Maintenance-Token`.

For single-node installs, the scheduled cleanup/reconcile intervals are usually enough:

```sh
QM_AUTH_SESSION_CLEANUP_INTERVAL_SECONDS=86400
QM_EXPIRY_REMINDER_RECONCILE_INTERVAL_SECONDS=3600
```

For multi-instance deployments, prefer one external scheduler or one API instance with those intervals enabled, plus the manual secret-protected endpoints for repair jobs. Prometheus should scrape `/internal/metrics` only through an internal network path or reverse proxy rule that injects or requires `X-QM-Maintenance-Token`; do not publish the internal routes as anonymous public endpoints.

## Invite Links And iOS Universal Links

If `QM_PUBLIC_BASE_URL` is set, it must be an `http://` or `https://` origin with no path, query, or fragment. Quartermaster uses it for browser-friendly invite links and mobile app setup codes. iOS Universal Links still require HTTPS and a matching associated-domain setup.

Passkeys are opt-in because native passkeys require a real HTTPS origin and app-domain association. To enable them, set `QM_PASSKEYS_ENABLED=true` and serve Quartermaster at the HTTPS `QM_PUBLIC_BASE_URL` users actually open. The server derives the WebAuthn RP ID and origin from that URL unless `QM_PASSKEY_RP_ID` or `QM_PASSKEY_ORIGIN` are set explicitly. The RP ID must match the domain associated with the native apps; local HTTP origins are only accepted for localhost development and are not useful for installed mobile passkeys.

For iOS Universal Links, configure:

| Variable           | Meaning                                        |
| ------------------ | ---------------------------------------------- |
| `QM_IOS_TEAM_ID`   | Apple Team ID used in the AASA payload         |
| `QM_IOS_BUNDLE_ID` | iOS bundle identifier used in the AASA payload |

For a local release identity setup, prefer the wrapper command:

```sh
cargo xtask configure-release-identity \
  --team YOUR_TEAM_ID \
  --bundle-id com.yourname.Quartermaster \
  --domain quartermaster.example.com
```

It writes the ignored iOS release config and prints the matching `QM_IOS_*` server environment to deploy with the same app identity.

The iOS app build also needs a matching associated-domain entitlement. The native app persists the server URL from setup/manual entry and reuses it on later launches. Without associated-domain setup, invite links still work through the browser fallback and manual invite entry.

iOS passkeys use the same associated domain through `webcredentials:<domain>` in Release entitlements and the server AASA payload. Android passkeys require the equivalent app/domain association through Android Digital Asset Links for the package and signing certificate you distribute.

Authenticated mobile handoff is separate from server setup links and invite links. A signed-in source device creates a short-lived QR payload containing the server URL plus a one-time handoff token, and the target device previews the source account before accepting. Expired, consumed, cancelled, or invalid handoff tokens fail closed. See [docs/authenticated-mobile-pairing.md](docs/authenticated-mobile-pairing.md).

## Clients

The native clients are not distributed through public app stores yet. For now, self-hosters build them from this repository:

- iOS setup lives in [ios/README.md](ios/README.md).
- Android setup lives in [android/README.md](android/README.md).
- The web shell is built into the container image and can also be built from `web/`.

## Contributing

Development setup, test commands, API generation, and repository conventions live in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

GNU Affero General Public License v3.0. See [LICENSE](LICENSE).
