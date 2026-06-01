# Supplier Cart Review Client Experience

Supplier automation can spend money or require account intervention, so cart
submission stays behind a review surface.

## Goals

- Make replenishment and supplier drafts understandable before submission.
- Keep grocery automation tied to kitchen replenishment, not general shopping
  or household errands.
- Show guardrail decisions, suppressions, estimated costs, supplier mappings,
  and human-intervention states.
- Require explicit user submission for review drafts.
- Keep receiving as an inventory action that writes stock batches through the
  normal stock ledger path.

## Flow

1. Generate or open a replenishment cart draft.
2. Show the cart run audit first:
   - guardrail decision: allowed, needs approval, or blocked.
   - suppressions with product and reason.
   - recommendations with supplier item, quantity, unit, price estimate, and
     automation level.
   - AI explanation only when present, redacted, and scoped to the run.
3. Open the supplier draft to review editable lines and intervention state.
4. Allow marking a draft ready only when the supplier reports no blocking
   intervention.
5. Submit the draft only from the review view.
6. Show submitted order status, redacted summary, review URL if available, and
   receive controls.
7. Receive order lines by asking for product, location, quantity, unit, expiry,
   and note, then call the supplier receive API.

## Client Responsibilities

- Never auto-submit from a route mount, push notification, or background refresh.
- Surface blocked guardrails as a terminal review state until the user changes
  rule/settings data and regenerates the run.
- Treat `human_intervention_required`, `login_required`,
  `browser_handoff_required`, and `manual_handoff_required` as user tasks, not
  errors.
- Display only redacted supplier summaries. Do not render or persist supplier
  secrets in UI state, AI task state, route params, logs, or test fixtures.
- For trusted auto-submit runs, still show the recorded decision and order
  state after the fact.

## Stable Selectors

Use these selectors or platform equivalents for smoke automation:

- Web `data-testid`
  - `cart-review-page`
  - `cart-generate`
  - `cart-guardrail-banner`
  - `cart-recommendation-row-{index}`
  - `cart-suppression-row-{index}`
  - `cart-draft-line-{line_id}`
  - `cart-submit`
  - `cart-order-result`
  - `cart-receive-line-{index}`
  - `cart-receive-submit`
- iOS accessibility identifiers
  - `cart.review`
  - `cart.generate`
  - `cart.guardrail.banner`
  - `cart.recommendation.row.{index}`
  - `cart.suppression.row.{index}`
  - `cart.draft.line.{line_id}`
  - `cart.submit`
  - `cart.order.result`
  - `cart.receive.submit`
- Android Compose test tags
  - `smoke-cook-screen`
  - `cart.generate`
  - `cart.row.{line_id}`
  - `cart.submit`
  - `cart.receive`

## Acceptance Criteria

- A household member can generate a mock replenishment cart draft, inspect why it
  needs approval, submit it, and see the order result.
- A household member can inspect a blocked cart run and understand which
  guardrail or suppression caused the block.
- Smoke automation can find every critical action by stable selectors rather
  than copy or coordinates.
