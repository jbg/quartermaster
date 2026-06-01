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
  let rangeStart = $state('');
  let rangeEnd = $state('');
  let dates = $state<string[]>([]);
  let includeBreakfast = $state(true);
  let includeLunch = $state(true);
  let includeDinner = $state(true);

  const maxRangeDays = 90;

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

  function parseDateInput(value: string): number | null {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
    if (!match) return null;
    const year = Number(match[1]);
    const month = Number(match[2]);
    const day = Number(match[3]);
    const timestamp = Date.UTC(year, month - 1, day);
    const parsed = new Date(timestamp);
    if (
      parsed.getUTCFullYear() !== year ||
      parsed.getUTCMonth() !== month - 1 ||
      parsed.getUTCDate() !== day
    ) {
      return null;
    }
    return timestamp;
  }

  function formatDateInput(timestamp: number): string {
    return new Date(timestamp).toISOString().slice(0, 10);
  }

  function rangeDates(): string[] | null {
    const start = parseDateInput(rangeStart);
    const end = parseDateInput(rangeEnd);
    if (start === null || end === null) {
      error = 'Choose a start and end date.';
      return null;
    }
    if (end < start) {
      error = 'End date must be on or after start date.';
      return null;
    }

    const dayMs = 24 * 60 * 60 * 1000;
    const dayCount = Math.floor((end - start) / dayMs) + 1;
    if (dayCount > maxRangeDays) {
      error = `Choose a range of ${maxRangeDays} days or fewer.`;
      return null;
    }

    return Array.from({ length: dayCount }, (_, index) => formatDateInput(start + index * dayMs));
  }

  function addRange() {
    const range = rangeDates();
    if (!range) return;
    dates = Array.from(new Set([...dates, ...range])).sort();
    rangeStart = '';
    rangeEnd = '';
    error = null;
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
    let planDates = dates;
    if (planDates.length === 0 && (rangeStart || rangeEnd)) {
      const pendingRange = rangeDates();
      if (!pendingRange) return;
      planDates = pendingRange;
      dates = pendingRange;
    }

    if (planDates.length === 0) {
      error = 'Add at least one date range.';
      return;
    }

    const slots = selectedSlots();
    if (slots.length === 0) {
      error = 'Choose at least one meal.';
      return;
    }

    busy = true;
    error = null;
    try {
      const plan = unwrapGenerated(
        await mealPlanGenerate({
          body: {
            title: title.trim() || null,
            dates: planDates,
            slots,
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
          novalidate
          aria-busy={busy}
        >
          <label>
            Title
            <input bind:value={title} placeholder="Next week at home" disabled={busy} />
          </label>
          <fieldset class="range-fieldset" disabled={busy}>
            <legend>Date range</legend>
            <div class="date-row">
              <label>
                Start
                <input type="date" bind:value={rangeStart} />
              </label>
              <label>
                End
                <input type="date" bind:value={rangeEnd} min={rangeStart || undefined} />
              </label>
              <button class="secondary-action" type="button" onclick={addRange}>Add range</button>
            </div>
          </fieldset>
          <div class="selected-dates">
            <div class="selected-dates-heading">
              <span>Selected dates</span>
              <strong>{dates.length}</strong>
            </div>
            {#if dates.length === 0}
              <p class="muted">Add a date range, then remove any dates you do not need.</p>
            {:else}
              <div class="chip-row" aria-label="Selected meal plan dates">
                {#each dates as date}
                  <button
                    class="date-chip"
                    type="button"
                    onclick={() => removeDate(date)}
                    disabled={busy}
                    aria-label={`Remove ${date}`}
                  >
                    {date} x
                  </button>
                {/each}
              </div>
            {/if}
          </div>
          <fieldset class="slot-fieldset" disabled={busy}>
            <legend>Meals</legend>
            <div class="slot-row">
              <label><input type="checkbox" bind:checked={includeBreakfast} /> Breakfast</label>
              <label><input type="checkbox" bind:checked={includeLunch} /> Lunch</label>
              <label><input type="checkbox" bind:checked={includeDinner} /> Dinner</label>
            </div>
          </fieldset>
          <button class="primary-action" type="submit" disabled={busy}>
            {busy ? 'Generating plan...' : 'Generate plan'}
          </button>
          {#if busy}
            <div class="generation-status" role="status" aria-live="polite">
              <span class="spinner" aria-hidden="true"></span>
              <div>
                <strong>Generating your meal plan</strong>
                <p class="muted">
                  This can take a little while as Quartermaster checks recipes and stock.
                </p>
              </div>
            </div>
          {/if}
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

  .range-fieldset,
  .slot-fieldset {
    border: 0;
    margin: 0;
    padding: 0;
  }

  .range-fieldset legend,
  .slot-fieldset legend,
  .selected-dates-heading {
    color: var(--qm-slate-700);
    font-weight: 800;
    margin-bottom: 0.45rem;
  }

  .date-row,
  .slot-row,
  .chip-row {
    align-items: end;
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }

  .date-row label {
    flex: 1 1 150px;
  }

  .selected-dates {
    border: 1px solid var(--qm-line);
    border-radius: 8px;
    padding: 0.8rem;
  }

  .selected-dates-heading {
    align-items: center;
    display: flex;
    justify-content: space-between;
    margin-bottom: 0.55rem;
  }

  .selected-dates-heading strong {
    color: var(--qm-green-900);
  }

  .date-chip,
  .plan-row,
  .generation-status {
    border: 1px solid var(--qm-line);
    border-radius: 8px;
    color: inherit;
    padding: 0.7rem 0.85rem;
    text-decoration: none;
  }

  .date-chip {
    background: var(--qm-sage-100);
  }

  .generation-status {
    align-items: center;
    background: var(--qm-color-surface-subtle);
    display: flex;
    gap: 0.75rem;
  }

  .generation-status p {
    margin: 0.15rem 0 0;
  }

  .spinner {
    animation: spin 0.8s linear infinite;
    border: 3px solid var(--qm-line-strong);
    border-top-color: var(--qm-green-800);
    border-radius: 999px;
    display: inline-block;
    flex: 0 0 auto;
    height: 24px;
    width: 24px;
  }

  .plan-row h3 {
    margin: 0;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 760px) {
    .meal-plan-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
