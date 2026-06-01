<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import AppFrame from '$lib/components/AppFrame.svelte';
  import { generatedTransport } from '$lib/api';
  import { recipeImportText, recipeList } from '$lib/generated/sdk.gen';
  import type { RecipeSummaryDto } from '$lib/generated/types.gen';
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
  let recipes = $state<RecipeSummaryDto[]>([]);
  let authenticated = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let importOpen = $state(false);
  let importName = $state('');
  let importText = $state('');
  let importBusy = $state(false);
  let importError = $state<string | null>(null);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const inventoryHref = $derived(appPath('/', page.url));
  const mealPlansHref = $derived(appPath('/meal-plans', page.url));
  const aiTasksHref = $derived(appPath('/ai/tasks', page.url));
  const cartReviewHref = $derived(appPath('/suppliers/review', page.url));

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
    void loadRecipes();
  });

  async function loadRecipes() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
      if (!currentHousehold(me)) {
        recipes = [];
        return;
      }
      recipes = unwrapGenerated(await recipeList()).items;
    } catch (err) {
      authenticated = false;
      me = null;
      recipes = [];
      error = err instanceof Error ? err.message : 'Sign in again to continue.';
    } finally {
      loading = false;
    }
  }

  async function switchHousehold(id: string) {
    if (!session) {
      return;
    }
    me = await session.switchHousehold(id);
    await loadRecipes();
  }

  async function importRecipe() {
    if (!importText.trim()) {
      importError = 'Paste recipe text before importing.';
      return;
    }
    importBusy = true;
    importError = null;
    try {
      const recipe = unwrapGenerated(
        await recipeImportText({
          body: {
            name: importName.trim() || null,
            text: importText,
            serving_count: '1',
            tags: ['imported']
          }
        })
      );
      await goto(appPath(`/recipes/${recipe.id}`, page.url));
    } catch (err) {
      importError = err instanceof Error ? err.message : 'Recipe import failed.';
    } finally {
      importBusy = false;
    }
  }

  async function logout() {
    if (!session) {
      return;
    }
    await session.logout();
    authenticated = false;
    me = null;
    recipes = [];
    await goto(inventoryHref);
  }
</script>

<svelte:head>
  <title>Recipes · Quartermaster</title>
</svelte:head>

<AppFrame
  title="Recipes"
  {authenticated}
  active="recipes"
  {activeHousehold}
  {households}
  onhouseholdchange={switchHousehold}
  onlogout={logout}
>
  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading recipes...</p>
    </section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <p class="muted">Open inventory and sign in before reviewing recipes.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      {#if error}<p class="error-text">{error}</p>{/if}
    </section>
  {:else}
    <section class="panel catalogue-panel">
      <div class="section-heading">
        <div>
          <p class="eyebrow">Cooking</p>
          <h2>Recipe library</h2>
        </div>
        <div class="action-row">
          <a class="secondary-action small" href={cartReviewHref}>Review shopping cart</a>
          <a class="secondary-action small" href={mealPlansHref}>Meal plans</a>
          <a class="secondary-action small" href={aiTasksHref}>AI activity</a>
          <button
            class="primary-action small"
            type="button"
            onclick={() => (importOpen = !importOpen)}
          >
            Import recipe text
          </button>
        </div>
      </div>

      {#if importOpen}
        <form
          class="phase8-form"
          onsubmit={(event) => {
            event.preventDefault();
            void importRecipe();
          }}
        >
          <label>
            Name
            <input bind:value={importName} data-testid="recipe-import-name" />
          </label>
          <label>
            Text
            <textarea bind:value={importText} rows="7" data-testid="recipe-import-text"></textarea>
          </label>
          <button
            class="primary-action"
            type="submit"
            disabled={importBusy}
            data-testid="recipe-import-submit"
          >
            {importBusy ? 'Importing...' : 'Import recipe'}
          </button>
          {#if importError}<p class="error-text">{importError}</p>{/if}
        </form>
      {/if}

      <div class="recipe-list" data-testid="recipe-list">
        {#if recipes.length === 0}
          <p class="muted">No recipes yet.</p>
        {:else}
          {#each recipes as recipe, index}
            <a
              class="recipe-row"
              href={appPath(`/recipes/${recipe.id}`, page.url)}
              data-testid={`recipe-row-${lineKey(recipe.id, index)}`}
            >
              <div>
                <h3>{recipe.name}</h3>
                <p class="muted">
                  {recipe.serving_count} servings · {recipe.source.replaceAll('_', ' ')}
                </p>
              </div>
              <span>{recipe.tags.join(', ')}</span>
            </a>
          {/each}
        {/if}
      </div>
    </section>
  {/if}
</AppFrame>

<style>
  .action-row,
  .recipe-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .action-row {
    flex-wrap: wrap;
  }

  .phase8-form {
    display: grid;
    gap: 0.85rem;
    margin: 1rem 0;
  }

  .phase8-form textarea {
    resize: vertical;
  }

  .recipe-list {
    display: grid;
    gap: 0.65rem;
  }

  .recipe-row {
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    color: inherit;
    padding: 0.85rem;
    text-decoration: none;
  }

  .recipe-row h3 {
    margin: 0;
  }
</style>
