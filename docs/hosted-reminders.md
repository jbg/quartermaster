# Hosted Reminder Operations

Quartermaster's reminder system has one supported v1 operating model:

- the backend owns reminder timing, wording, and inbox state
- API pods own stock mutations, the durable inbox API, and enqueueing bounded background jobs
- worker pods own leased background jobs plus APNs and FCM delivery attempts
- Postgres is the hosted coordination point for job leases and idempotent state transitions

Use this guide when running reminders in a hosted deployment with multiple API and worker pods.

## Deployment Shapes

Run API pods and worker pods as separate processes:

- API process: `cargo run -p qm-server`
- worker process: `cargo run -p qm-server -- worker`

Hosted mode should use Postgres. SQLite is still useful for local single-node development, but it is not the multi-pod worker coordination target.

## Production-Shaped Example

Example environment split:

- API process
  - `QM_PUBLIC_BASE_URL=https://quartermaster.example.com`
  - `QM_WEB_AUTH_ALLOWED_ORIGINS=https://app.quartermaster.example.com` when the browser shell is served from a separate origin
  - `QM_RATE_LIMIT_CLIENT_IP_MODE=x-forwarded-for`
  - `QM_RATE_LIMIT_TRUSTED_PROXY_CIDRS=10.0.0.0/24`
  - `QM_DATABASE_URL=postgres://...`
  - `QM_EXPIRY_REMINDERS_ENABLED=true`
  - `QM_EXPIRY_REMINDER_RECONCILE_INTERVAL_SECONDS=300`
  - `QM_EXPIRY_REMINDER_TRIGGER_SECRET=<shared-maintenance-secret>`
  - `QM_AUTH_SESSION_CLEANUP_INTERVAL_SECONDS=300`
  - `QM_AUTH_SESSION_SWEEP_TRIGGER_SECRET=<shared-maintenance-secret>`
  - `QM_METRICS_ENABLED=true`
  - `QM_METRICS_TRIGGER_SECRET=<shared-maintenance-secret>`
- worker process
  - `QM_DATABASE_URL=postgres://...`
  - `QM_WORKER_POLL_INTERVAL_SECONDS=30`
  - `QM_WORKER_BATCH_SIZE=25`
  - `QM_WORKER_LEASE_TTL_SECONDS=60`
  - `QM_WORKER_RETRY_BACKOFF_SECONDS=300`
  - `QM_APNS_ENABLED=true`
  - `QM_APNS_ENVIRONMENT=production`
  - `QM_APNS_TOPIC=com.example.quartermaster`
  - `QM_APNS_KEY_ID=<apple-key-id>`
  - `QM_APNS_TEAM_ID=<apple-team-id>`
  - `QM_APNS_PRIVATE_KEY_PATH=/run/secrets/quartermaster-apns.p8`
  - `QM_FCM_ENABLED=true`
  - `QM_FCM_PROJECT_ID=<firebase-project-id>`
  - `QM_FCM_SERVICE_ACCOUNT_JSON_PATH=/run/secrets/quartermaster-fcm-service-account.json`
  - `QM_METRICS_ENABLED=true`
  - `QM_METRICS_BIND=127.0.0.1:9091`
  - `QM_METRICS_TRIGGER_SECRET=<shared-maintenance-secret>`

Example maintenance calls:

```sh
curl -X POST \
  -H "X-QM-Maintenance-Token: $QM_AUTH_SESSION_SWEEP_TRIGGER_SECRET" \
  https://quartermaster.example.com/internal/maintenance/sweep-auth-sessions

curl -X POST \
  -H "X-QM-Maintenance-Token: $QM_EXPIRY_REMINDER_TRIGGER_SECRET" \
  https://quartermaster.example.com/internal/maintenance/sweep-expiry-reminders
```

The maintenance endpoints enqueue background jobs. They are useful for drift repair and low-frequency maintenance, but the worker process must be running to drain the queued work.

Use one scheduling owner for each periodic maintenance path. In a single-node install that can be the API process itself. In a multi-pod deployment, either enable the interval on one API pod only or call the manual endpoints from one external scheduler. Running the same reconcile/sweep interval on every API pod is safe but noisy because each process enqueues repair work independently.

## Reverse Proxies and Browser Origins

Credentialed browser auth only needs CORS when the browser origin differs from the API origin. Keep `QM_WEB_AUTH_ALLOWED_ORIGINS` unset when Quartermaster serves the web shell and API from the same host. For a split deployment, list the exact HTTPS browser origins:

```sh
QM_PUBLIC_BASE_URL=https://api.quartermaster.example.com
QM_WEB_AUTH_ALLOWED_ORIGINS=https://app.quartermaster.example.com,https://kitchen.example.net
```

Entries must be origins only: scheme, host, and optional port, with no path, query, or fragment.

`X-Forwarded-For` is accepted only when both proxy settings are explicit:

```sh
QM_RATE_LIMIT_CLIENT_IP_MODE=x-forwarded-for
QM_RATE_LIMIT_TRUSTED_PROXY_CIDRS=10.42.0.0/16,fd00:42::/64
```

Set the CIDRs to the actual reverse proxy or ingress source ranges. If Quartermaster is exposed directly, leave the mode as `socket`.

## Metrics and Health

When metrics are enabled, Quartermaster exposes secret-header-protected internal routes:

- API process: `GET /internal/metrics`
- worker process: `GET /internal/metrics` and `GET /healthz`

All internal scrape and maintenance routes require the same secret-header pattern:

- header: `X-QM-Maintenance-Token`
- value: the configured trigger secret for that surface

Do not expose these routes publicly without a reverse proxy or network boundary that keeps them internal.

Prometheus can scrape through a small internal proxy that adds the maintenance token before forwarding to Quartermaster:

```nginx
location /quartermaster/metrics {
    allow 10.50.0.0/16;
    deny all;
    proxy_set_header X-QM-Maintenance-Token $quartermaster_metrics_token;
    proxy_pass http://quartermaster-api:8080/internal/metrics;
}
```

For split worker mode, scrape the API process on its normal bind and each worker on `QM_METRICS_BIND`. The worker bind defaults to loopback, so either run the scraper sidecar on the same host/pod or bind it to an internal-only interface such as `0.0.0.0:9091` behind network policy.

## What Healthy Delivery Looks Like

Healthy reminder delivery usually looks like:

- `qm_push_worker_last_cycle_completed_timestamp_seconds` keeps advancing
- `qm_reminders_oldest_due_age_seconds` stays low
- `qm_push_deliveries_active_claim_count` spikes briefly during a cycle and returns to zero
- `qm_push_deliveries_retry_due_count` stays near zero most of the time
- `qm_push_devices_with_invalid_token_count` stays near zero

## What To Check When Something Looks Wrong

If no reminders appear due:

- confirm `QM_EXPIRY_REMINDERS_ENABLED=true`
- confirm batches actually have `expires_on`
- confirm the household timezone is set correctly
- enqueue expiry reminder reconcile jobs once to repair drift

If due reminders keep backing up:

- confirm the worker is running at all
- check that `qm_push_worker_last_cycle_completed_timestamp_seconds` is moving
- check worker logs for APNs or FCM transport/auth failures
- confirm the worker can reach Apple and Google push endpoints from the deployment network

If retry backlog keeps growing:

- check `qm_push_deliveries_retry_due_count`
- inspect worker logs for repeated transient provider or transport errors
- verify the APNs environment/topic or FCM project/service-account config match the client build you actually shipped

If invalid tokens keep growing:

- check `qm_push_devices_with_invalid_token_count`
- expect reminders for those exact tokens to stop retrying
- confirm iOS clients are re-registering devices after reinstall, sign-in, or token refresh

If active claims stay elevated:

- check whether the worker is stuck mid-cycle or crashing after claiming work
- compare `qm_push_deliveries_active_claim_count` with recent worker logs
- wait for claim TTL expiry or run a later cycle to let stale claims roll into retryable state

If metrics appear missing:

- verify `QM_METRICS_ENABLED=true`
- verify the scrape request sends `X-QM-Maintenance-Token`
- verify the worker's `QM_METRICS_BIND` is reachable from the scraper

## Provider Requirements and Failure Modes

Quartermaster's current provider contract is intentionally small:

- `QM_APNS_ENABLED=true`
- `QM_APNS_TOPIC=<bundle identifier>`
- APNs auth uses either:
  - `QM_APNS_AUTH_TOKEN=<bearer token>`
  - `QM_APNS_KEY_ID`, `QM_APNS_TEAM_ID`, and exactly one of `QM_APNS_PRIVATE_KEY_PATH` or `QM_APNS_PRIVATE_KEY`
- `QM_APNS_ENVIRONMENT=sandbox|production`
- optional local/provider testing override: `QM_APNS_BASE_URL`
- `QM_FCM_ENABLED=true`
- `QM_FCM_PROJECT_ID=<firebase-project-id>`
- FCM auth uses exactly one of `QM_FCM_SERVICE_ACCOUNT_JSON_PATH` or `QM_FCM_SERVICE_ACCOUNT_JSON`
- optional local/provider testing overrides: `QM_FCM_BASE_URL` and `QM_FCM_TOKEN_URL`

`QM_APNS_AUTH_TOKEN` is still supported for operators that already mint provider tokens externally. When it is set, Quartermaster uses it directly and ignores APNs JWT signing fields.

Common failure classes:

- transport failure: no provider response; reminder/device goes retryable
- retryable provider failure: APNs or FCM responded with a transient error; reminder/device gets `next_retry_at`
- permanent provider failure: APNs or FCM rejected the token or request permanently; that token stops retrying for that reminder

An invalid or unregistered token does not remove the reminder from the durable inbox. It only stops retrying that reminder for that exact token.

## Identity and Universal-Link Alignment

Quartermaster supports one explicit v1 identity story:

- one associated-domain host
- one iOS app identity pairing checked by `cargo xtask verify-release-config`

Keep these aligned:

- iOS release config resolves `QUARTERMASTER_ASSOCIATED_DOMAIN=quartermaster.example.com`
- backend AASA route serves `https://quartermaster.example.com/.well-known/apple-app-site-association`
- `QM_IOS_TEAM_ID` + `QM_IOS_BUNDLE_ID` match `QUARTERMASTER_IOS_DEVELOPMENT_TEAM` + `QUARTERMASTER_IOS_BUNDLE_ID`

After touching any of those values, run:

```sh
cargo xtask verify-release-config
```

## Verification Workflow

Use this minimum checklist after reminder-hosting changes:

```sh
cargo test --workspace
```

If you changed reminder delivery or deployment wiring, also validate split worker mode locally:

1. Run the API normally.
2. Run `cargo run -p qm-server -- worker`.
3. Confirm `GET /internal/metrics` and worker `/healthz` behave as expected.
4. Exercise one reminder end-to-end with a registered iOS device or a local APNs test setup.
