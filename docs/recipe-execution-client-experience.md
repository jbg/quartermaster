# Recipe Execution Client Experience

Cooking is an explicit review flow. A recipe can suggest inventory changes, but
the client must show the plan before any stock ledger mutation is submitted.
This covers saved recipes, imported recipes, and pantry/AI suggestions that
become executable recipe plans.

## Goals

- Show recipes as household data with clear source and provenance.
- Support inventory-aware cooking and meal planning without introducing a
  general household task list or calendar.
- Turn "cook this" into a reviewable plan: matched batches, missing
  ingredients, conversion assumptions, optional substitutions, and produced
  stock.
- Require an explicit confirmation before `POST /recipes/executions`.
- Keep retry-safe execution by always sending an idempotency key.
- Make post-action history readable by linking users back to the affected stock
  batches and stock history.

## Flow

1. List recipes with name, tags, serving count, and source.
2. Open a recipe detail view with ingredients, outputs, steps, validation, and
   provenance.
3. Run preflight with the recipe id/version id, serving scale, selected
   substitutions, and `use_expiring_first`.
4. Render the preflight result as a review plan:
   - `can_execute=true`: primary action is enabled.
   - missing optional ingredients: show as warnings.
   - missing required ingredients: disable execution until the user explicitly
     confirms partial execution.
   - conversion assumptions: show near the affected ingredient.
   - matched batches: show product, location, quantity, expiry, and depleted
     state.
   - outputs: show produced stock preview before execution.
5. Execute with the same adjusted request plus a generated idempotency key.
6. Show success with execution id, consumed batch count, output batches, and a
   link back to inventory/history.

## Client Responsibilities

- Never call execution from a list row or background refresh.
- Keep generated or imported recipe text visible as supporting human context;
  execution must use the structured ingredient rows returned by the API.
- Use plain language for uncertainty:
  - "Matched" for deterministic product or ingredient mappings.
  - "Assumption" for conversion or substitution logic.
  - "Missing" for required unavailable ingredients.
- Display provenance without exposing secrets or raw provider credentials.
- Use the current household session recovery path for `403` responses.

## Stable Selectors

Use these selectors or platform equivalents for smoke automation:

- Web `data-testid`
  - `recipe-list`
  - `recipe-row-{recipe_id}`
  - `recipe-import-text`
  - `recipe-import-submit`
  - `recipe-preflight-run`
  - `recipe-preflight-row-{line_id_or_index}`
  - `recipe-missing-row-{line_id_or_index}`
  - `recipe-partial-confirm`
  - `recipe-execute`
  - `recipe-execution-result`
- iOS accessibility identifiers
  - `recipe.list`
  - `recipe.row.{recipe_id}`
  - `recipe.preflight.run`
  - `recipe.preflight.row.{line_id_or_index}`
  - `recipe.missing.row.{line_id_or_index}`
  - `recipe.partial.confirm`
  - `recipe.preflight.execute`
  - `recipe.execution.result`
- Android Compose test tags
  - `smoke-cook-screen`
  - `recipe.row.{recipe_id}`
  - `recipe.preflight.row.{line_id_or_index}`
  - `recipe.missing.row.{line_id_or_index}`
  - `recipe.preflight.execute`

## Acceptance Criteria

- A household member can inspect an executable recipe, run preflight, review the
  exact batches to consume, execute it, and see a success state.
- A household member can inspect a recipe with a missing required ingredient and
  see why execution is blocked until partial execution is explicitly confirmed.
- The same fixture-backed flow is smoke-testable on web, iOS, and Android.
