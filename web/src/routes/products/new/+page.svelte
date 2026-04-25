<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { generatedTransport } from '$lib/api';
  import { unitChoicesForFamily } from '$lib/inventory';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type MeResponse,
    type UnitFamily
  } from '$lib/session-core';
  import {
    buildProductCreateRequest,
    emptyProductForm,
    productMutationErrorMessage,
    setProductFormFamily,
    validateProductForm
  } from '$lib/products';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let form = $state(emptyProductForm());

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const unitChoices = $derived(unitChoicesForFamily(form.family));

  onMount(() => {
    if (!browser) {
      return;
    }
    const created = new QuartermasterSession(
      createBrowserSessionStorage(window.localStorage, window.location),
      generatedTransport()
    );
    session = created;
    authenticated = Boolean(created.snapshot().accessToken);
    if (!authenticated) {
      loading = false;
      return;
    }
    void loadSession();
  });

  async function loadSession() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
    } catch {
      authenticated = false;
      me = null;
      error = 'Sign in again to continue.';
    } finally {
      loading = false;
    }
  }

  function updateFamily(family: string) {
    form = setProductFormFamily(form, family as UnitFamily);
  }

  async function createProduct() {
    if (!session) {
      return;
    }
    const validation = validateProductForm(form);
    if (validation) {
      error = validation;
      return;
    }
    busy = true;
    error = null;
    try {
      const product = await session.productCreate(buildProductCreateRequest(form));
      await goto(`/products/${product.id}`);
    } catch (err) {
      error = productMutationErrorMessage(err, 'Product could not be created.');
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title>New Product · Quartermaster</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">Products</p>
      <h1>New Product</h1>
    </div>
    <div class="heading-actions">
      <a class="secondary-action" href="/products">Products</a>
      <a class="secondary-action" href="/">Inventory</a>
    </div>
  </header>

  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading...</p>
    </section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <p class="muted">Open the inventory screen and sign in before creating products.</p>
      <a class="primary-action" href="/">Go to inventory</a>
      {#if error}
        <p class="error-text">{error}</p>
      {/if}
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p class="muted">Switch to a household from the inventory screen before creating products.</p>
      <a class="primary-action" href="/">Go to inventory</a>
    </section>
  {:else}
    <section class="panel product-form-panel">
      <form
        class="product-management-form"
        onsubmit={(event) => {
          event.preventDefault();
          void createProduct();
        }}
      >
        <label>
          Product name
          <input bind:value={form.name} data-testid="product-name-input" maxlength="256" required />
        </label>
        <label>
          Brand
          <input bind:value={form.brand} data-testid="product-brand-input" />
        </label>
        <label>
          Product family
          <select
            value={form.family}
            data-testid="product-family-select"
            onchange={(event) => updateFamily(event.currentTarget.value)}
          >
            <option value="mass">Mass</option>
            <option value="volume">Volume</option>
            <option value="count">Count</option>
          </select>
        </label>
        <label>
          Preferred unit
          <select bind:value={form.preferredUnit} data-testid="product-unit-select">
            {#each unitChoices as unit}
              <option value={unit}>{unit}</option>
            {/each}
          </select>
        </label>
        <label>
          Image URL
          <input bind:value={form.imageUrl} data-testid="product-image-url-input" />
        </label>
        {#if error}
          <p class="error-text">{error}</p>
        {/if}
        <div class="row-actions">
          <button class="primary-action" type="submit" disabled={busy} data-testid="product-create">
            {busy ? 'Creating...' : 'Create product'}
          </button>
          <a class="secondary-action" href="/products">Cancel</a>
        </div>
      </form>
    </section>
  {/if}
</main>
