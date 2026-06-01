<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import AppFrame from '$lib/components/AppFrame.svelte';
  import { generatedTransport } from '$lib/api';
  import { mealPlanGenerate, mealPlanList } from '$lib/generated/sdk.gen';
  import type { MealPlanSummaryDto, MealSlotDto } from '$lib/generated/types.gen';
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
  let plans = $state<MealPlanSummaryDto[]>([]);
  let authenticated = $state(false);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let title = $state('');
  let dateInput = $state('');
  let dates = $state<string[]>([]);
  let includeBreakfast = $state(true);
  let includeLunch = $state(true);
  let includeDinner = $state(true);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const inventoryHref = $derived(appPath('/', page.url));
  const recipesHref = $derived(appPath('/recipes', page.url));

  onMount(() => {
    if (!browser) return;
    session = new QuartermasterSession(
      createBrowserSessionStorage(window.localStorage, window.location),
      generatedTransport()
    );
    authenticated = true;
    void loadPlans();
  });

  async function loadPlans() {
    if (!session) return;
    loading = true;
    error = null;
    try {
      me = await session.me();
      if (!currentHousehold(me)) {
        plans = [];
        return;
      }
      plans = unwrapGenerated(await mealPlanList()).items;
    } catch (err) {
      authenticated = false;
      me = null;
      plans = [];
      error = err instanceof Error ? err.message : 'Sign in again to continue.';
    } finally {
      loading = false;
    }
  }

  function addDate() {
    if (!dateInput || dates.includes(dateInput)) return;
    dates = [...dates, dateInput].sort();
    dateInput = '';
  }

  function removeDate(date: string) {
    dates = dates.filter((item) => item !== date);
  }

  function selectedSlots(): MealSlotDto[] {
    const slots: MealSlotDto[] = [];
    if (includeBreakfast) slots.push({ key: 'breakfast', label: 'Breakfast' });
    if (includeLunch) slots.push({ key: 'lunch', label: 'Lunch' });
    if (includeDinner) slots.push({ key: 'dinner', label: 'Dinner' });
    return slots;
  }

  async function generatePlan() {
    if (dates.length === 0) {
      error = 'Add at least one date.';
      return;
    }
    busy = true;
    error = null;
    try {
      const plan = unwrapGenerated(
        await mealPlanGenerate({
          body: {
            title: title.trim() || null,
            dates,
            slots: selectedSlots(),
            constraints: {}
          }
        })
      );
      await goto(appPath(`/meal-plans/${plan.id}`, page.url));
    } catch (err) {
      error = err instanceof Error ? err.message : 'Meal plan generation failed.';
    } finally {
      busy = false;
    }
  }

  async function switchHousehold(id: string) {
    if (!session) return;
    me = await session.switchHousehold(id);
    await loadPlans();
  }

  async function logout() {
    if (!session) return;
    await session.logout();
    authenticated = false;
    me = null;
    plans = [];
    await goto(inventoryHref);
  }
</script>

<svelte:head>
  <title>Meal Plans - Quartermaster</title>
</svelte:head>

<AppFrame
  title="Meal Plans"
  {authenticated}
  active="meal-plans"
  {activeHousehold}
  {households}
  onhouseholdchange={switchHousehold}
  onlogout={logout}
>
  {#if loading}
    <section class="panel empty-state"><p class="muted">Loading meal plans...</p></section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      {#if error}<p class="error-text">{error}</p>{/if}
    </section>
  {:else}
    <section class="meal-plan-grid">
      <section class="panel meal-plan-panel">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Plan ahead</p>
            <h2>Generate from dates</h2>
          </div>
          <a class="secondary-action small" href={recipesHref}>Recipes</a>
        </div>
        <form
          class="plan-form"
          onsubmit={(event) => {
            event.preventDefault();
            void generatePlan();
          }}
        >
          <label>
            Title
            <input bind:value={title} placeholder="Next week at home" />
          </label>
          <div class="date-row">
            <label>
              Date
              <input type="date" bind:value={dateInput} />
            </label>
            <button class="secondary-action" type="button" onclick={addDate}>Add date</button>
          </div>
          <div class="chip-row">
            {#each dates as date}
              <button class="date-chip" type="button" onclick={() => removeDate(date)}>
                {date} x
              </button>
            {/each}
          </div>
          <div class="slot-row">
            <label><input type="checkbox" bind:checked={includeBreakfast} /> Breakfast</label>
            <label><input type="checkbox" bind:checked={includeLunch} /> Lunch</label>
            <label><input type="checkbox" bind:checked={includeDinner} /> Dinner</label>
          </div>
          <button class="primary-action" type="submit" disabled={busy}>
            {busy ? 'Generating...' : 'Generate plan'}
          </button>
          {#if error}<p class="error-text">{error}</p>{/if}
        </form>
      </section>

      <section class="panel meal-plan-panel">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Saved</p>
            <h2>Household plans</h2>
          </div>
        </div>
        <div class="plan-list">
          {#if plans.length === 0}
            <p class="muted">No meal plans yet.</p>
          {:else}
            {#each plans as plan}
              <a class="plan-row" href={appPath(`/meal-plans/${plan.id}`, page.url)}>
                <div>
                  <h3>{plan.title}</h3>
                  <p class="muted">
                    {plan.dates.join(', ')} - {plan.meal_count} meals - {plan.status}
                  </p>
                </div>
              </a>
            {/each}
          {/if}
        </div>
      </section>
    </section>
  {/if}
</AppFrame>

<style>
  .meal-plan-grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: minmax(320px, 0.75fr) minmax(0, 1fr);
  }

  .plan-form,
  .plan-list {
    display: grid;
    gap: 0.8rem;
  }

  .date-row,
  .slot-row,
  .chip-row {
    align-items: end;
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }

  .date-chip,
  .plan-row {
    border: 1px solid var(--qm-line);
    border-radius: 8px;
    color: inherit;
    padding: 0.7rem 0.85rem;
    text-decoration: none;
  }

  .date-chip {
    background: var(--qm-sage-100);
  }

  .plan-row h3 {
    margin: 0;
  }

  @media (max-width: 760px) {
    .meal-plan-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
