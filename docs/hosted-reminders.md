# Hosted Reminder Operations

Quartermaster's reminder system has one supported v1 operating model:

- the backend owns reminder timing, wording, and inbox state
- the API process owns stock mutations, reminder reconciliation, and the durable inbox API
- the push worker owns APNs and FCM delivery attempts
- the reminder sweeper is a repair tool, not the primary delivery loop

Use this guide when running reminders in a self-hosted or small hosted deployment.

## Deployment Shapes

You can run the worker in either supported shape:

- integrated mode: run `cargo run -p qm-server` with `QM_PUSH_WORKER_ENABLED=true`
- split mode: run the API normally and start a second process with `cargo run -p qm-server -- push-worker`

Split mode is the recommended hosted shape because it keeps reminder delivery work isolated from the main API process while preserving the same database-backed coordination model.

## Production-Shaped Example

Example environment split:

- API process
  - `QM_PUBLIC_BASE_URL=https://quartermaster.example.com`
  - `QM_EXPIRY_REMINDERS_ENABLED=true`
  - `QM_EXPIRY_REMINDER_TRIGGER_SECRET=<shared-maintenance-secret>`
  - `QM_AUTH_SESSION_SWEEP_TRIGGER_SECRET=<shared-maintenance-secret>`
  - `QM_METRICS_ENABLED=true`
  - `QM_METRICS_TRIGGER_SECRET=<shared-maintenance-secret>`
- worker process
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

The sweeper endpoints are useful for drift repair and low-frequency maintenance. They are not a replacement for the running push worker.

## Metrics and Health

When metrics are enabled, Quartermaster exposes secret-header-protected internal routes:

- API process: `GET /internal/metrics`
- worker process: `GET /internal/metrics` and `GET /healthz`

All internal scrape and maintenance routes require the same secret-header pattern:

- header: `X-QM-Maintenance-Token`
- value: the configured trigger secret for that surface

Do not expose these routes publicly without a reverse proxy or network boundary that keeps them internal.

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
- run the expiry reminder sweep once to repair drift

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
- `QM_FCM_ENABLED=true`
- `QM_FCM_PROJECT_ID=<firebase-project-id>`
- FCM auth uses exactly one of `QM_FCM_SERVICE_ACCOUNT_JSON_PATH` or `QM_FCM_SERVICE_ACCOUNT_JSON`

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
2. Run `cargo run -p qm-server -- push-worker`.
3. Confirm `GET /internal/metrics` and worker `/healthz` behave as expected.
4. Exercise one reminder end-to-end with a registered iOS device or a local APNs test setup.
