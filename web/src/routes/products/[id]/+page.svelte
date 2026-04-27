<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import { generatedTransport } from '$lib/api';
  import { productPreferredUnit } from '$lib/inventory';
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
    productBarcode,
    productBrand,
    productDeletedAt,
    productImageUrl,
    productMutationErrorMessage,
    productSourceLabel
  } from '$lib/products';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let actionBusy = $state(false);
  let product = $state<Product | null>(null);
  let error = $state<string | null>(null);
  let actionError = $state<string | null>(null);

  const productId = $derived(page.params.id ?? '');
  const activeHousehold = $derived(me ? currentHousehold(me) : null);

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
    actionError = null;
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

  async function restoreProduct() {
    if (!session || !product) {
      return;
    }
    actionBusy = true;
    actionError = null;
    try {
      product = await session.productRestore(product.id);
    } catch (err) {
      actionError = productMutationErrorMessage(err, 'Product could not be restored.');
    } finally {
      actionBusy = false;
    }
  }

  async function refreshProduct() {
    if (!session || !product) {
      return;
    }
    actionBusy = true;
    actionError = null;
    try {
      product = await session.productRefresh(product.id);
    } catch (err) {
      actionError = productMutationErrorMessage(err, 'Product could not be refreshed.');
    } finally {
      actionBusy = false;
    }
  }

  async function logout() {
    if (!session) {
      return;
    }
    await session.logout();
    authenticated = false;
    me = null;
    product = null;
    await goto('/');
  }
</script>

<svelte:head>
  <title>{product ? `${product.name} · Quartermaster` : 'Product · Quartermaster'}</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div class="brand-heading">
      <img class="brand-mark" src="/brand/quartermaster-mark.svg" alt="" />
      <div>
        <p class="eyebrow">Product</p>
        <h1>{product?.name ?? 'Product'}</h1>
      </div>
    </div>
    <div class="heading-actions">
      <a class="secondary-action" href="/products">Products</a>
      <a class="secondary-action" href="/">Inventory</a>
      {#if authenticated}
        <button class="ghost-button" type="button" onclick={logout}>Log out</button>
      {/if}
    </div>
  </header>

  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading product...</p>
    </section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <p class="muted">Open the inventory screen and sign in before viewing products.</p>
      <a class="primary-action" href="/">Go to inventory</a>
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p class="muted">Switch to a household from the inventory screen before viewing products.</p>
      <a class="primary-action" href="/">Go to inventory</a>
    </section>
  {:else if error || !product}
    <section class="panel empty-state">
      <h2>Product unavailable</h2>
      <p class="muted">{error ?? 'Product could not be found.'}</p>
      <a class="primary-action" href="/products">Back to products</a>
    </section>
  {:else}
    <section class="product-detail-layout">
      <section class="panel product-detail-panel">
        <div class="product-detail-heading">
          {#if productImageUrl(product)}
            <img src={productImageUrl(product)} alt="" />
          {:else}
            <div class="product-image-placeholder large" aria-hidden="true">
              {product.name.slice(0, 1)}
            </div>
          {/if}
          <div>
            <p class="eyebrow">{productSourceLabel(product)}</p>
            <h2>{product.name}</h2>
            <p class="muted">{productBrand(product) || 'No brand'}</p>
          </div>
        </div>

        <dl class="detail-grid">
          <div>
            <dt>Family</dt>
            <dd>{product.family}</dd>
          </div>
          <div>
            <dt>Preferred unit</dt>
            <dd>{productPreferredUnit(product)}</dd>
          </div>
          <div>
            <dt>Barcode</dt>
            <dd>{productBarcode(product) || 'No barcode'}</dd>
          </div>
          <div>
            <dt>Status</dt>
            <dd>{isDeletedProduct(product) ? `Deleted ${productDeletedAt(product)}` : 'Active'}</dd>
          </div>
          <div>
            <dt>Image URL</dt>
            <dd>{productImageUrl(product) || 'No image'}</dd>
          </div>
        </dl>

        <div class="row-actions">
          {#if isManualProduct(product) && !isDeletedProduct(product)}
            <a class="primary-action" href={`/products/${product.id}/edit`}>Edit product</a>
            <a class="ghost-button danger" href={`/products/${product.id}/delete`}>Delete</a>
          {:else if isManualProduct(product) && isDeletedProduct(product)}
            <button
              class="primary-action"
              type="button"
              disabled={actionBusy}
              data-testid="product-restore"
              onclick={restoreProduct}
            >
              {actionBusy ? 'Restoring...' : 'Restore product'}
            </button>
          {:else}
            <button
              class="secondary-action"
              type="button"
              disabled={actionBusy}
              onclick={refreshProduct}
            >
              {actionBusy ? 'Refreshing...' : 'Refresh from OpenFoodFacts'}
            </button>
          {/if}
        </div>
        {#if actionError}
          <p class="error-text">{actionError}</p>
        {/if}
      </section>
    </section>
  {/if}
</main>
