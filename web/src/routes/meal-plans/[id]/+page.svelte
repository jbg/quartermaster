<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import AppFrame from '$lib/components/AppFrame.svelte';
  import { generatedTransport } from '$lib/api';
  import {
    mealPlanGet,
    mealPlanMealExecute,
    mealPlanMealSkip,
    mealPlanRefresh
  } from '$lib/generated/sdk.gen';
  import type { MealPlanDto } from '$lib/generated/types.gen';
  import { unwrapGenerated } from '$lib/phase8';
  import { appPath } from '$lib/paths';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type MeResponse
  } from '$lib/session-core';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let plan = $state<MealPlanDto | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let busyMeal = $state<string | null>(null);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const planId = $derived(page.params.id ?? '');
  const inventoryHref = $derived(appPath('/', page.url));
  const plansHref = $derived(appPath('/meal-plans', page.url));

  onMount(() => {
    if (!browser) return;
    session = new QuartermasterSession(
      createBrowserSessionStorage(window.localStorage, window.location),
      generatedTransport()
    );
    authenticated = true;
    void loadPlan();
  });

  async function loadPlan() {
    if (!session) return;
    loading = true;
    error = null;
    try {
      me = await session.me();
      plan = unwrapGenerated(await mealPlanGet({ path: { id: planId } }));
    } catch (err) {
      authenticated = false;
      error = err instanceof Error ? err.message : 'Meal plan could not be loaded.';
    } finally {
      loading = false;
    }
  }

  async function refreshPlan() {
    error = null;
    success = null;
    try {
      const response = unwrapGenerated(await mealPlanRefresh({ path: { id: planId } }));
      plan = response.plan;
      success = 'Reservations refreshed.';
    } catch (err) {
      error = err instanceof Error ? err.message : 'Refresh failed.';
    }
  }

  async function cookMeal(mealId: string) {
    busyMeal = mealId;
    error = null;
    success = null;
    try {
      const response = unwrapGenerated(
        await mealPlanMealExecute({ path: { id: planId, meal_id: mealId } })
      );
      success = `Cooked meal with execution ${response.execution_id.slice(0, 8)}.`;
      plan = unwrapGenerated(await mealPlanGet({ path: { id: planId } }));
    } catch (err) {
      error = err instanceof Error ? err.message : 'Cook failed.';
    } finally {
      busyMeal = null;
    }
  }

  async function skipMeal(mealId: string) {
    busyMeal = mealId;
    error = null;
    success = null;
    try {
      plan = unwrapGenerated(await mealPlanMealSkip({ path: { id: planId, meal_id: mealId } }));
      success = 'Meal skipped and reservations released.';
    } catch (err) {
      error = err instanceof Error ? err.message : 'Skip failed.';
    } finally {
      busyMeal = null;
    }
  }

  async function switchHousehold(id: string) {
    if (!session) return;
    me = await session.switchHousehold(id);
    await loadPlan();
  }

  async function logout() {
    if (!session) return;
    await session.logout();
    authenticated = false;
    me = null;
    plan = null;
    await goto(inventoryHref);
  }
</script>

<svelte:head>
  <title>{plan?.title ?? 'Meal Plan'} - Quartermaster</title>
</svelte:head>

<AppFrame
  title={plan?.title ?? 'Meal Plan'}
  {authenticated}
  active="meal-plans"
  {activeHousehold}
  {households}
  onhouseholdchange={switchHousehold}
  onlogout={logout}
>
  {#if loading}
    <section class="panel empty-state"><p class="muted">Loading meal plan...</p></section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      {#if error}<p class="error-text">{error}</p>{/if}
    </section>
  {:else if plan}
    <section class="panel">
      <div class="section-heading">
        <div>
          <p class="eyebrow">{plan.status}</p>
          <h2>{plan.days.length} planned dates</h2>
        </div>
        <div class="actions">
          <a class="secondary-action small" href={plansHref}>All plans</a>
          <button class="primary-action small" type="button" onclick={refreshPlan}>
            Refresh reservations
          </button>
        </div>
      </div>
      {#if success}<p class="success-text">{success}</p>{/if}
      {#if error}<p class="error-text">{error}</p>{/if}
    </section>

    <section class="plan-days">
      {#each plan.days as day}
        <section class="panel day-panel">
          <div class="section-heading">
            <div>
              <p class="eyebrow">Date</p>
              <h2>{day.date}</h2>
            </div>
          </div>
          <div class="meal-list">
            {#each day.meals as meal}
              <article class:conflicted={meal.status === 'conflicted'} class="meal-row">
                <div>
                  <p class="eyebrow">{meal.slot_label} - {meal.status}</p>
                  <h3>{meal.recipe_name ?? 'Unassigned meal'}</h3>
                  {#if meal.preflight}
                    <p class="muted">
                      {meal.preflight.ingredients.length} ingredients -
                      {meal.reservations.length} reservations
                    </p>
                  {/if}
                  {#each meal.warnings as warning}
                    <p class="warning-text">{warning}</p>
                  {/each}
                  {#each meal.conflicts as conflict}
                    <p class="error-text">{conflict}</p>
                  {/each}
                </div>
                <div class="meal-actions">
                  <button
                    class="primary-action small"
                    type="button"
                    disabled={busyMeal === meal.id || meal.status !== 'planned'}
                    onclick={() => cookMeal(meal.id)}
                  >
                    {busyMeal === meal.id ? 'Working...' : 'Cook'}
                  </button>
                  <button
                    class="secondary-action small"
                    type="button"
                    disabled={busyMeal === meal.id || meal.status === 'skipped'}
                    onclick={() => skipMeal(meal.id)}
                  >
                    Skip
                  </button>
                </div>
              </article>
            {/each}
          </div>
        </section>
      {/each}
    </section>
  {/if}
</AppFrame>

<style>
  .actions,
  .meal-actions,
  .meal-row {
    align-items: center;
    display: flex;
    gap: 0.75rem;
  }

  .actions {
    flex-wrap: wrap;
  }

  .plan-days,
  .meal-list {
    display: grid;
    gap: 1rem;
  }

  .meal-row {
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    justify-content: space-between;
    padding: 0.85rem;
  }

  .meal-row h3 {
    margin: 0.15rem 0;
  }

  .meal-row.conflicted {
    border-color: var(--danger-border, #e28b8b);
  }

  .meal-actions {
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .warning-text {
    color: #9a5b00;
    margin: 0.25rem 0 0;
  }

  @media (max-width: 720px) {
    .meal-row {
      align-items: stretch;
      flex-direction: column;
    }

    .meal-actions {
      justify-content: flex-start;
    }
  }
</style>
