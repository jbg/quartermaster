<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import { generatedTransport } from '$lib/api';
  import { appPath } from '$lib/paths';
  import { unitChoicesForFamily } from '$lib/inventory';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type MeResponse,
    type Product,
    type Unit,
    type UnitFamily
  } from '$lib/session-core';
  import {
    buildProductUpdateRequest,
    isDeletedProduct,
    isManualProduct,
    productFormFields,
    productMutationErrorMessage,
    setProductFormFamily,
    validateProductForm,
    type ProductFormFields
  } from '$lib/products';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let busy = $state(false);
  let product = $state<Product | null>(null);
  let form = $state<ProductFormFields | null>(null);
  let error = $state<string | null>(null);
  let units = $state<Unit[]>([]);

  const productId = $derived(page.params.id ?? '');
  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const unitChoices = $derived(form ? unitChoicesForFamily(form.family, units) : []);
  const inventoryHref = $derived(appPath('/', page.url));
  const productsHref = $derived(appPath('/products', page.url));
  const productHref = $derived(
    product ? appPath(`/products/${product.id}`, page.url) : productsHref
  );
  const brandMarkSrc = $derived(appPath('/brand/quartermaster-mark.svg', page.url));

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
    void loadProduct();
  });

  async function loadProduct() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
      if (currentHousehold(me) && productId) {
        units = await session.unitsList().catch(() => []);
        product = await session.productGet(productId);
        form = productFormFields(product, units);
      }
    } catch {
      product = null;
      form = null;
      authenticated = false;
      error = 'Product could not be loaded.';
    } finally {
      loading = false;
    }
  }

  function updateFamily(family: string) {
    if (!form) {
      return;
    }
    form = setProductFormFamily(form, family as UnitFamily, units);
  }

  async function saveProduct() {
    if (!session || !product || !form) {
      return;
    }
    const validation = validateProductForm(form, units);
    if (validation) {
      error = validation;
      return;
    }
    busy = true;
    error = null;
    try {
      const updated = await session.productUpdate(
        product.id,
        buildProductUpdateRequest(product, form)
      );
      await goto(appPath(`/products/${updated.id}`, page.url));
    } catch (err) {
      error = productMutationErrorMessage(err, 'Product could not be saved.');
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title>{product ? `Edit ${product.name} · Quartermaster` : 'Edit Product · Quartermaster'}</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div class="brand-heading">
      <img class="brand-mark" src={brandMarkSrc} alt="" />
      <div>
        <p class="eyebrow">Products</p>
        <h1>Edit Product</h1>
      </div>
    </div>
    <div class="heading-actions">
      <a class="secondary-action" href={productHref}>Product</a>
      <a class="secondary-action" href={productsHref}>Products</a>
    </div>
  </header>

  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading product...</p>
    </section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <p class="muted">Open the inventory screen and sign in before editing products.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p class="muted">Switch to a household from the inventory screen before editing products.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
    </section>
  {:else if error && (!product || !form)}
    <section class="panel empty-state">
      <h2>Product unavailable</h2>
      <p class="muted">{error}</p>
      <a class="primary-action" href={productsHref}>Back to products</a>
    </section>
  {:else if product && form && (!isManualProduct(product) || isDeletedProduct(product))}
    <section class="panel empty-state">
      <h2>Product is read-only</h2>
      <p class="muted">Only active manual products can be edited.</p>
      <a class="primary-action" href={productHref}>Back to product</a>
    </section>
  {:else if product && form}
    <section class="panel product-form-panel">
      <form
        class="product-management-form"
        onsubmit={(event) => {
          event.preventDefault();
          void saveProduct();
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
          <button class="primary-action" type="submit" disabled={busy} data-testid="product-save">
            {busy ? 'Saving...' : 'Save product'}
          </button>
          <a class="secondary-action" href={productHref}>Cancel</a>
        </div>
      </form>
    </section>
  {/if}
</main>
