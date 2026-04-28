<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import { generatedTransport } from '$lib/api';
  import { appPath } from '$lib/paths';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type MeResponse,
    type Product
  } from '$lib/session-core';
  import {
    isDeletedProduct,
    isManualProduct,
    productBrand,
    productMutationErrorMessage
  } from '$lib/products';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let busy = $state(false);
  let product = $state<Product | null>(null);
  let error = $state<string | null>(null);

  const productId = $derived(page.params.id ?? '');
  const activeHousehold = $derived(me ? currentHousehold(me) : null);
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
        product = await session.productGet(productId);
      }
    } catch {
      product = null;
      authenticated = false;
      error = 'Product could not be loaded.';
    } finally {
      loading = false;
    }
  }

  async function deleteProduct() {
    if (!session || !product) {
      return;
    }
    busy = true;
    error = null;
    try {
      await session.productDelete(product.id);
      await goto(appPath('/products?include=deleted', page.url));
    } catch (err) {
      error = productMutationErrorMessage(err, 'Product could not be deleted.');
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title
    >{product ? `Delete ${product.name} · Quartermaster` : 'Delete Product · Quartermaster'}</title
  >
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div class="brand-heading">
      <img class="brand-mark" src={brandMarkSrc} alt="" />
      <div>
        <p class="eyebrow">Products</p>
        <h1>Delete Product</h1>
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
      <p class="muted">Open the inventory screen and sign in before deleting products.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p class="muted">Switch to a household from the inventory screen before deleting products.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
    </section>
  {:else if error && !product}
    <section class="panel empty-state">
      <h2>Product unavailable</h2>
      <p class="muted">{error}</p>
      <a class="primary-action" href={productsHref}>Back to products</a>
    </section>
  {:else if product && (!isManualProduct(product) || isDeletedProduct(product))}
    <section class="panel empty-state">
      <h2>Product cannot be deleted</h2>
      <p class="muted">Only active manual products can be deleted.</p>
      <a class="primary-action" href={productHref}>Back to product</a>
    </section>
  {:else if product}
    <section class="panel product-form-panel">
      <div class="delete-confirmation">
        <h2>Delete {product.name}?</h2>
        <p class="muted">
          {productBrand(product) || 'Manual product'} will be hidden from new stock creation. It can
          be restored later.
        </p>
        <div class="row-actions">
          <button
            class="ghost-button danger"
            type="button"
            disabled={busy}
            data-testid="product-delete-confirm"
            onclick={deleteProduct}
          >
            {busy ? 'Deleting...' : 'Delete product'}
          </button>
          <a class="secondary-action" href={productHref}>Cancel</a>
        </div>
        {#if error}
          <p class="error-text">{error}</p>
        {/if}
      </div>
    </section>
  {/if}
</main>
