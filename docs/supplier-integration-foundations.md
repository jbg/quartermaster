# Supplier Integration Foundations

Quartermaster's supplier boundary is established without binding the project to
a real grocery, restaurant, wholesaler, or browser checkout flow yet. The first
implementation is intentionally in-tree and mock-backed: it gives API clients,
database migrations, the job runner, and tests a real contract while avoiding
spend, credential, and terms-of-service risk before a concrete supplier is
chosen.

## Decision

Supplier integrations implement the Rust `SupplierIntegration` trait in
`qm-suppliers`. The trait covers catalog search/detail, cart draft validation,
order submission, order status, cancellation, receiving hints, and explicit
human-intervention states. Quartermaster owns household scoping, credentials,
audit records, retries, browser-session records, debug-artifact redaction, and
final inventory mutation.

The database keeps supplier enum-like values as `TEXT`, matching the existing
portable boundary pattern. Public API DTOs expose typed enums for capabilities,
account status, secret kind, mapping confidence, cart status, order status,
intervention state, and debug artifact kind.

## Credential And Safety Boundaries

Supplier credentials are household-scoped and encrypted before storage with
`QM_SUPPLIER_CREDENTIAL_ENCRYPTION_KEY`. API responses expose only metadata and
redacted hints. Plaintext credentials must not be logged, returned, serialized
into AI prompts, written into task records, or captured in supplier debug
artifacts.

Supplier failures surface as redacted, structured errors. Browser-only suppliers
must pass through the controlled browser/session abstraction and may record only
redacted screenshot or HTML artifacts. A supplier flow that needs login,
consent, site recovery, or manual checkout must move into a human-intervention
state instead of guessing.

## Runner Model

The background job table reserves supplier cart-submit and order-status-sync
job kinds. The runner foundation uses existing job leases, retry backoff, and
worker logging. Per-supplier rate limits, circuit breaker state, and richer
browser orchestration can attach to these job kinds as real integrations are
added.

Receiving is not a supplier-side mutation. When a user receives an order,
Quartermaster creates stock batches through the existing stock helper path so
the stock ledger remains the source of truth.

## Real Supplier Deferral

No real external supplier ships today. The deterministic mock supplier is the
executable contract for tests and client development. A first real thin
integration should be selected only after the supplier, region, account model,
terms, credential shape, and browser/API requirements are known.

## Future Plugin Shape

Keeping the first implementation in-tree does not preclude plugins. The current
trait is deliberately explicit about capabilities, required secrets,
configuration, supported regions, terms URL, network needs, browser needs, and
human intervention. If in-tree integrations become constraining, this contract
can be adapted to a dynamically loaded or out-of-process plugin protocol without
changing Quartermaster's ownership of credentials, audit, jobs, and stock
mutation.
