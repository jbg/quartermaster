<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import AppFrame from '$lib/components/AppFrame.svelte';
  import { generatedTransport } from '$lib/api';
  import {
    recipeExecutionExecute,
    recipeExecutionPreflight,
    recipeGet
  } from '$lib/generated/sdk.gen';
  import type {
    RecipeDto,
    RecipeExecutionPreflightResponse,
    RecipeExecutionRequest,
    RecipeExecutionResponse
  } from '$lib/generated/types.gen';
  import { lineKey, unwrapGenerated } from '$lib/phase8';
  import { appPath } from '$lib/paths';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type MeResponse
  } from '$lib/session-core';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let recipe = $state<RecipeDto | null>(null);
  let plan = $state<RecipeExecutionPreflightResponse | null>(null);
  let result = $state<RecipeExecutionResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let allowPartial = $state(false);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const recipeId = $derived(page.params.id);
  const inventoryHref = $derived(appPath('/', page.url));
  const recipesHref = $derived(appPath('/recipes', page.url));

  onMount(() => {
    if (!browser) {
      return;
    }
    const created = new QuartermasterSession(
      createBrowserSessionStorage(window.localStorage, window.location),
      generatedTransport()
    );
    session = created;
    authenticated = true;
    void loadRecipe();
  });

  function executionRequest(partial = allowPartial): RecipeExecutionRequest {
    if (!recipe) {
      throw new Error('Recipe is not loaded.');
    }
    return {
      recipe_id: recipe.id,
      recipe_version_id: recipe.version.id,
      recipe_name: recipe.name,
      serving_scale: '1',
      use_expiring_first: true,
      allow_partial: partial,
      ingredients: recipe.version.ingredients
        .filter((ingredient) => ingredient.quantity.amount && ingredient.quantity.unit)
        .map((ingredient) => ({
          line_id: ingredient.id ?? ingredient.display_name,
          display_name: ingredient.display_name,
          ingredient_id: ingredient.ingredient_id ?? null,
          product_id: ingredient.product_id ?? null,
          quantity: ingredient.quantity.amount ?? '0',
          unit: ingredient.quantity.unit ?? 'piece',
          optional: ingredient.optional ?? false,
          preparation: ingredient.preparation ?? null,
          substitution_of: null,
          location_id: null
        })),
      outputs: []
    };
  }

  async function loadRecipe() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
      recipe = unwrapGenerated(await recipeGet({ path: { id: recipeId ?? '' } }));
    } catch (err) {
      authenticated = false;
      error = err instanceof Error ? err.message : 'Recipe could not be loaded.';
    } finally {
      loading = false;
    }
  }

  async function runPreflight() {
    busy = true;
    error = null;
    result = null;
    try {
      plan = unwrapGenerated(await recipeExecutionPreflight({ body: executionRequest(false) }));
      allowPartial = false;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Preflight failed.';
    } finally {
      busy = false;
    }
  }

  async function executeRecipe() {
    busy = true;
    error = null;
    try {
      const request = executionRequest(allowPartial);
      request.idempotency_key = crypto.randomUUID();
      result = unwrapGenerated(await recipeExecutionExecute({ body: request }));
      plan = result.plan;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Recipe execution failed.';
    } finally {
      busy = false;
    }
  }

  async function switchHousehold(id: string) {
    if (!session) {
      return;
    }
    me = await session.switchHousehold(id);
    await loadRecipe();
  }

  async function logout() {
    if (!session) {
      return;
    }
    await session.logout();
    authenticated = false;
    me = null;
    await goto(inventoryHref);
  }
</script>

<svelte:head>
  <title>{recipe?.name ?? 'Recipe'} · Quartermaster</title>
</svelte:head>

<AppFrame
  title={recipe?.name ?? 'Recipe'}
  {authenticated}
  active="recipes"
  {activeHousehold}
  {households}
  onhouseholdchange={switchHousehold}
  onlogout={logout}
>
  {#if loading}
    <section class="panel empty-state"><p class="muted">Loading recipe...</p></section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      {#if error}<p class="error-text">{error}</p>{/if}
    </section>
  {:else if recipe}
    <section class="recipe-detail-grid">
      <section class="panel">
        <div class="section-heading">
          <div>
            <p class="eyebrow">{recipe.source.replaceAll('_', ' ')}</p>
            <h2>Structured recipe</h2>
          </div>
          <a class="secondary-action small" href={recipesHref}>All recipes</a>
        </div>
        <p class="muted">{recipe.description ?? 'No description'}</p>
        <h3>Ingredients</h3>
        <ul class="compact-list">
          {#each recipe.version.ingredients as ingredient}
            <li>
              <strong>{ingredient.display_name}</strong>
              <span>
                {ingredient.quantity.amount ?? 'to taste'}
                {ingredient.quantity.unit ?? ''}
                {ingredient.optional ? ' optional' : ''}
              </span>
            </li>
          {/each}
        </ul>
        <h3>Steps</h3>
        <ol class="compact-list">
          {#each recipe.version.steps as step}
            <li>{step.instruction}</li>
          {/each}
        </ol>
        <h3>Provenance</h3>
        <ul class="compact-list">
          {#each recipe.version.provenance as item}
            <li>
              {item.source_type.replaceAll('_', ' ')}
              {item.parser_confidence ? `· confidence ${item.parser_confidence}` : ''}
            </li>
          {/each}
        </ul>
      </section>

      <section class="panel">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Review plan</p>
            <h2>Preflight</h2>
          </div>
          <button
            class="primary-action small"
            type="button"
            onclick={runPreflight}
            disabled={busy}
            data-testid="recipe-preflight-run"
          >
            {busy ? 'Checking...' : 'Check stock'}
          </button>
        </div>

        {#if plan}
          <p class:success-text={plan.can_execute} class:error-text={!plan.can_execute}>
            {plan.can_execute ? 'Ready to cook.' : 'Required ingredients are missing.'}
          </p>
          <div class="review-list">
            {#each plan.ingredients as ingredient, index}
              <article
                class="review-row"
                data-testid={`recipe-preflight-row-${lineKey(ingredient.line_id, index)}`}
              >
                <h3>{ingredient.display_name ?? ingredient.product.name}</h3>
                <p>
                  {ingredient.requested_quantity}
                  {ingredient.requested_unit} requested · {ingredient.inventory_quantity}
                  {ingredient.inventory_unit} planned
                </p>
                {#if ingredient.conversion_assumption}<p class="muted">
                    {ingredient.conversion_assumption}
                  </p>{/if}
                {#each ingredient.matched_batches as batch}
                  <p class="muted">
                    Batch {batch.batch_id.slice(0, 8)} · {batch.quantity}
                    {batch.unit}{batch.expires_on ? ` · expires ${batch.expires_on}` : ''}
                  </p>
                {/each}
              </article>
            {/each}
            {#each plan.missing_ingredients as missing, index}
              <article
                class="review-row warning"
                data-testid={`recipe-missing-row-${lineKey(missing.line_id, index)}`}
              >
                <h3>{missing.display_name ?? 'Missing ingredient'}</h3>
                <p>
                  {missing.missing_quantity}
                  {missing.requested_unit} missing · {missing.reason}
                </p>
              </article>
            {/each}
          </div>
          {#if !plan.can_execute}
            <label class="checkbox-row">
              <input
                type="checkbox"
                bind:checked={allowPartial}
                data-testid="recipe-partial-confirm"
              />
              Confirm partial execution
            </label>
          {/if}
          <button
            class="primary-action"
            type="button"
            onclick={executeRecipe}
            disabled={busy || (!plan.can_execute && !allowPartial)}
            data-testid="recipe-execute"
          >
            {busy ? 'Cooking...' : 'Cook recipe'}
          </button>
        {:else}
          <p class="muted">Run preflight to review the inventory plan before cooking.</p>
        {/if}
        {#if result}
          <div class="success-box" data-testid="recipe-execution-result">
            <h3>Recipe cooked</h3>
            <p>Execution {result.execution_id}</p>
            <p>{result.plan.ingredients.length} ingredients reviewed.</p>
          </div>
        {/if}
        {#if error}<p class="error-text">{error}</p>{/if}
      </section>
    </section>
  {/if}
</AppFrame>

<style>
  .recipe-detail-grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: minmax(0, 1fr) minmax(320px, 0.9fr);
  }

  .compact-list,
  .review-list {
    display: grid;
    gap: 0.55rem;
  }

  .review-row {
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 0.75rem;
  }

  .review-row h3 {
    margin: 0 0 0.25rem;
  }

  .warning {
    border-color: var(--warning-border, #b87900);
  }

  .checkbox-row {
    display: flex;
    gap: 0.5rem;
    margin: 1rem 0;
  }

  .success-box {
    border: 1px solid var(--success-border, #2f855a);
    border-radius: 8px;
    margin-top: 1rem;
    padding: 0.75rem;
  }

  @media (max-width: 840px) {
    .recipe-detail-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
