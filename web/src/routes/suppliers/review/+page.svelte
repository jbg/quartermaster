<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import AppFrame from '$lib/components/AppFrame.svelte';
  import { generatedTransport } from '$lib/api';
  import { replenishmentCartDraftCreate, replenishmentCartRunGet } from '$lib/generated/sdk.gen';
  import type {
    ReplenishmentCartRunDto,
    ReplenishmentCreateCartDraftResponse,
    SupplierCartDraftDto,
    SupplierOrderDto
  } from '$lib/generated/types.gen';
  import { apiFetch, jsonPreview, lineKey, unwrapGenerated } from '$lib/phase8';
  import { appPath } from '$lib/paths';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type Location,
    type MeResponse
  } from '$lib/session-core';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let locations = $state<Location[]>([]);
  let run = $state<ReplenishmentCartRunDto | null>(null);
  let draft = $state<SupplierCartDraftDto | null>(null);
  let order = $state<SupplierOrderDto | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let receiveLocationId = $state('');
  let receiveExpiresOn = $state('');

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const inventoryHref = $derived(appPath('/', page.url));
  const recipesHref = $derived(appPath('/recipes', page.url));
  const recommendations = $derived(asArray(run?.recommendations));
  const suppressions = $derived(asArray(run?.suppressions));
  const firstReceivableLine = $derived(draft?.lines.find((line) => line.product_id) ?? null);

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
    void loadInitial();
  });

  function asArray(value: unknown): Array<Record<string, unknown>> {
    return Array.isArray(value) ? (value as Array<Record<string, unknown>>) : [];
  }

  function labelFromValue(value: string): string {
    return value.replaceAll('_', ' ');
  }

  function cartDecisionLabel(decision: ReplenishmentCartRunDto['guardrail_decision']): string {
    switch (decision) {
      case 'allowed':
        return 'Ready to submit';
      case 'needs_approval':
        return 'Needs your approval';
      case 'blocked':
        return 'Blocked';
    }
  }

  function cartRunStatusLabel(status: ReplenishmentCartRunDto['status']): string {
    switch (status) {
      case 'draft_created':
        return 'Suggested cart created';
      case 'blocked':
        return 'No cart created';
      case 'submitted':
        return 'Order submitted';
    }
  }

  function cartDraftStatusLabel(status: SupplierCartDraftDto['status']): string {
    switch (status) {
      case 'draft':
        return 'Draft';
      case 'needs_review':
        return 'Needs review';
      case 'ready':
        return 'Ready for review';
      case 'submitted':
        return 'Submitted';
      case 'cancelled':
        return 'Cancelled';
    }
  }

  async function loadInitial() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
      locations = await session.locationsList();
      receiveLocationId = locations[0]?.id ?? '';
      const draftId = page.url.searchParams.get('draft');
      const runId = page.url.searchParams.get('run');
      if (runId) {
        run = unwrapGenerated(await replenishmentCartRunGet({ path: { id: runId } }));
      }
      if (draftId) {
        draft = await apiFetch<SupplierCartDraftDto>(
          session,
          `/api/v1/suppliers/cart-drafts/${draftId}`
        );
      }
    } catch (err) {
      authenticated = false;
      error = err instanceof Error ? err.message : 'Shopping cart could not be loaded.';
    } finally {
      loading = false;
    }
  }

  async function generateCart() {
    if (!session) {
      return;
    }
    busy = true;
    error = null;
    order = null;
    try {
      const response: ReplenishmentCreateCartDraftResponse = unwrapGenerated(
        await replenishmentCartDraftCreate({
          body: { supplier_id: 'mock', include_ai_explanation: true }
        })
      );
      run = response.run;
      draft = response.draft_id
        ? await apiFetch<SupplierCartDraftDto>(
            session,
            `/api/v1/suppliers/cart-drafts/${response.draft_id}`
          )
        : null;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Cart generation failed.';
    } finally {
      busy = false;
    }
  }

  async function submitCart() {
    if (!session || !draft) {
      return;
    }
    busy = true;
    error = null;
    try {
      order = await apiFetch<SupplierOrderDto>(
        session,
        `/api/v1/suppliers/cart-drafts/${draft.id}/submit`,
        { method: 'POST' }
      );
      draft = { ...draft, status: 'submitted' };
    } catch (err) {
      error = err instanceof Error ? err.message : 'Cart submission failed.';
    } finally {
      busy = false;
    }
  }

  async function receiveOrder() {
    if (!session || !order || !firstReceivableLine?.product_id || !receiveLocationId) {
      return;
    }
    busy = true;
    error = null;
    try {
      order = await apiFetch<SupplierOrderDto>(
        session,
        `/api/v1/suppliers/orders/${order.id}/receive`,
        {
          method: 'POST',
          body: JSON.stringify({
            lines: [
              {
                product_id: firstReceivableLine.product_id,
                location_id: receiveLocationId,
                quantity: '1000',
                unit: 'g',
                expires_on: receiveExpiresOn.trim() || null,
                note: 'received from web shopping cart'
              }
            ]
          })
        }
      );
    } catch (err) {
      error = err instanceof Error ? err.message : 'Order receive failed.';
    } finally {
      busy = false;
    }
  }

  async function switchHousehold(id: string) {
    if (!session) {
      return;
    }
    me = await session.switchHousehold(id);
    await loadInitial();
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
  <title>Shopping · Quartermaster</title>
</svelte:head>

<AppFrame
  title="Shopping"
  eyebrow="Ordering"
  {authenticated}
  active="automation"
  {activeHousehold}
  {households}
  onhouseholdchange={switchHousehold}
  onlogout={logout}
>
  {#if loading}
    <section class="panel empty-state"><p class="muted">Loading shopping cart...</p></section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      {#if error}<p class="error-text">{error}</p>{/if}
    </section>
  {:else}
    <section class="cart-grid" data-testid="cart-review-page">
      <section class="panel cart-review-panel">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Suggestions</p>
            <h2>Suggested cart</h2>
          </div>
          <div class="action-row">
            <a class="secondary-action small" href={recipesHref}>Recipes</a>
            <button
              class="primary-action small"
              type="button"
              onclick={generateCart}
              disabled={busy}
              data-testid="cart-generate"
            >
              {busy ? 'Building...' : 'Build cart'}
            </button>
          </div>
        </div>
        {#if run}
          <div class="guardrail" data-testid="cart-guardrail-banner">
            <strong>{cartDecisionLabel(run.guardrail_decision)}</strong>
            <span>{cartRunStatusLabel(run.status)}</span>
          </div>
          <h3>Suggested items</h3>
          <div class="review-list">
            {#each recommendations as recommendation, index}
              <article class="review-row" data-testid={`cart-recommendation-row-${index}`}>
                <strong>{recommendation.supplier_item_id}</strong>
                <p>{recommendation.quantity} {recommendation.unit}</p>
                <p class="muted">
                  {recommendation.estimated_price_amount ?? 'unknown price'}
                  {recommendation.estimated_price_currency ?? ''}
                </p>
              </article>
            {/each}
          </div>
          <h3>Skipped items</h3>
          {#if suppressions.length === 0}
            <p class="muted">None</p>
          {:else}
            {#each suppressions as suppression, index}
              <p class="muted" data-testid={`cart-suppression-row-${index}`}>
                {jsonPreview(suppression)}
              </p>
            {/each}
          {/if}
          {#if run.ai_explanation}
            <h3>Why these items</h3>
            <pre>{jsonPreview(run.ai_explanation)}</pre>
          {/if}
        {:else}
          <p class="muted">Build a suggested cart from current stock and replenishment rules.</p>
        {/if}
      </section>

      <section class="panel cart-review-panel">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Supplier</p>
            <h2>Cart to approve</h2>
          </div>
          <button
            class="primary-action small"
            type="button"
            onclick={submitCart}
            disabled={busy || !draft || draft.status === 'submitted'}
            data-testid="cart-submit"
          >
            {busy ? 'Submitting...' : 'Submit cart'}
          </button>
        </div>
        {#if draft}
          <p>
            {draft.supplier_id} · {cartDraftStatusLabel(draft.status)}
            {#if draft.intervention_state !== 'none'}
              · {labelFromValue(draft.intervention_state)}
            {/if}
          </p>
          <div class="review-list">
            {#each draft.lines as line}
              <article class="review-row" data-testid={`cart-draft-line-${line.id}`}>
                <strong>{line.supplier_item_id}</strong>
                <p>{line.quantity} {line.unit ?? ''}</p>
                {#if line.note}<p class="muted">{line.note}</p>{/if}
              </article>
            {/each}
          </div>
        {:else}
          <p class="muted">No cart is ready for review.</p>
        {/if}

        {#if order}
          <div class="success-box" data-testid="cart-order-result">
            <h3>Order {order.status.replaceAll('_', ' ')}</h3>
            <p>{order.supplier_order_id ?? order.id}</p>
            <pre>{jsonPreview(order.redacted_summary)}</pre>
          </div>
          <div class="receive-form">
            <label>
              Receive location
              <select bind:value={receiveLocationId}>
                {#each locations as location}
                  <option value={location.id}>{location.name}</option>
                {/each}
              </select>
            </label>
            <label>
              Expiry
              <input
                bind:value={receiveExpiresOn}
                placeholder="YYYY-MM-DD"
                data-testid="cart-receive-line-0"
              />
            </label>
            <button
              class="secondary-action"
              type="button"
              onclick={receiveOrder}
              disabled={busy || order.status === 'delivered'}
              data-testid="cart-receive-submit"
            >
              Receive order
            </button>
          </div>
        {/if}
        {#if error}<p class="error-text">{error}</p>{/if}
      </section>
    </section>
  {/if}
</AppFrame>

<style>
  .cart-grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: minmax(0, 1fr) minmax(320px, 0.85fr);
    margin-top: 22px;
  }

  .cart-review-panel {
    padding: var(--qm-space-5);
  }

  .action-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .guardrail,
  .review-row,
  .success-box {
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 0.75rem;
  }

  .guardrail {
    display: flex;
    justify-content: space-between;
    margin-bottom: 1rem;
  }

  .review-list,
  .receive-form {
    display: grid;
    gap: 0.65rem;
  }

  pre {
    overflow: auto;
    white-space: pre-wrap;
  }

  @media (max-width: 840px) {
    .cart-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
