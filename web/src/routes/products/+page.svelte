<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { generatedTransport } from '$lib/api';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type MeResponse,
    type Product
  } from '$lib/session-core';
  import {
    barcodeLookupErrorMessage,
    filterDeletedProducts,
    includeDeletedForFilter,
    isDeletedProduct,
    parseProductInclude,
    productBarcode,
    productBrand,
    productImageUrl,
    productListHref,
    productSourceLabel,
    type ProductIncludeFilter
  } from '$lib/products';
  import { productPreferredUnit } from '$lib/inventory';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let products = $state<Product[]>([]);
  let error = $state<string | null>(null);
  let searchQuery = $state('');
  let includeFilter = $state<ProductIncludeFilter>('active');
  let barcodeLookupValue = $state('');
  let barcodeLookupBusy = $state(false);
  let barcodeLookupError = $state<string | null>(null);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const visibleProducts = $derived(filterDeletedProducts(products, includeFilter));

  onMount(() => {
    if (!browser) {
      return;
    }
    const params = new URLSearchParams(window.location.search);
    searchQuery = params.get('q') ?? '';
    includeFilter = parseProductInclude(params.get('include'));
    const created = new QuartermasterSession(
      createBrowserSessionStorage(window.localStorage, window.location),
      generatedTransport()
    );
    session = created;
    authenticated = true;
    void loadProducts();
  });

  async function loadProducts() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
      if (!currentHousehold(me)) {
        products = [];
        return;
      }
      const response = await session.productList({
        q: searchQuery.trim() || null,
        limit: 100,
        include_deleted: includeDeletedForFilter(includeFilter)
      });
      products = response.items ?? [];
    } catch {
      me = null;
      products = [];
      authenticated = false;
      error = 'Sign in again to continue.';
    } finally {
      loading = false;
    }
  }

  async function applyFilters() {
    const href = productListHref(searchQuery, includeFilter);
    await goto(href);
    await loadProducts();
  }

  async function lookupBarcodeProduct() {
    if (!session) {
      return;
    }
    const barcode = barcodeLookupValue.trim();
    if (!barcode) {
      return;
    }
    barcodeLookupBusy = true;
    barcodeLookupError = null;
    try {
      const response = await session.productByBarcode(barcode);
      await goto(`/products/${response.product.id}`);
    } catch (err) {
      barcodeLookupError = barcodeLookupErrorMessage(err);
    } finally {
      barcodeLookupBusy = false;
    }
  }

  async function logout() {
    if (!session) {
      return;
    }
    await session.logout();
    authenticated = false;
    me = null;
    products = [];
    await goto('/');
  }
</script>

<svelte:head>
  <title>Products · Quartermaster</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">Quartermaster</p>
      <h1>Products</h1>
    </div>
    <div class="heading-actions">
      <a class="secondary-action" href="/">Inventory</a>
      <a class="secondary-action" href="/settings">Settings</a>
      {#if authenticated}
        <button class="ghost-button" type="button" onclick={logout}>Log out</button>
      {/if}
    </div>
  </header>

  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading products...</p>
    </section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <p class="muted">Open the inventory screen and sign in before managing products.</p>
      <a class="primary-action" href="/">Go to inventory</a>
      {#if error}
        <p class="error-text">{error}</p>
      {/if}
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p class="muted">Switch to a household from the inventory screen before managing products.</p>
      <a class="primary-action" href="/">Go to inventory</a>
    </section>
  {:else}
    <section class="catalogue-layout">
      <section class="panel catalogue-panel" aria-labelledby="product-list-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Catalogue</p>
            <h2 id="product-list-heading">Product list</h2>
          </div>
          <a class="primary-action small" href="/products/new">New product</a>
        </div>

        <form
          class="catalogue-filters"
          onsubmit={(event) => {
            event.preventDefault();
            void applyFilters();
          }}
        >
          <label>
            Search
            <input bind:value={searchQuery} data-testid="product-search-input" />
          </label>
          <label>
            Include
            <select bind:value={includeFilter} data-testid="product-include-filter">
              <option value="active">Active</option>
              <option value="all">All</option>
              <option value="deleted">Deleted</option>
            </select>
          </label>
          <button class="secondary-action" type="submit" data-testid="product-filter-apply">
            Apply
          </button>
        </form>

        <form
          class="barcode-catalogue-lookup"
          data-testid="products-barcode-lookup"
          onsubmit={(event) => {
            event.preventDefault();
            void lookupBarcodeProduct();
          }}
        >
          <label>
            Barcode lookup
            <input
              bind:value={barcodeLookupValue}
              data-testid="products-barcode-lookup-input"
              inputmode="numeric"
              placeholder="EAN or UPC"
            />
          </label>
          <button
            class="secondary-action"
            type="submit"
            data-testid="products-barcode-lookup-submit"
            disabled={!barcodeLookupValue.trim() || barcodeLookupBusy}
          >
            {barcodeLookupBusy ? 'Looking up...' : 'Look up'}
          </button>
        </form>

        {#if barcodeLookupError}
          <p class="error-text">{barcodeLookupError}</p>
        {/if}

        {#if error}
          <p class="error-text">{error}</p>
        {/if}

        {#if visibleProducts.length === 0}
          <p class="muted">No products found.</p>
        {:else}
          <div class="product-catalogue-list" data-testid="product-catalogue-list">
            {#each visibleProducts as product}
              <article
                class:deleted={isDeletedProduct(product)}
                class="product-catalogue-row"
                data-testid={`product-row-${product.name}`}
              >
                {#if productImageUrl(product)}
                  <img src={productImageUrl(product)} alt="" />
                {:else}
                  <div class="product-image-placeholder" aria-hidden="true">
                    {product.name.slice(0, 1)}
                  </div>
                {/if}
                <div>
                  <h3>
                    <a href={`/products/${product.id}`}>{product.name}</a>
                  </h3>
                  <p>
                    {productBrand(product) || 'No brand'} · {product.family} · {productPreferredUnit(
                      product
                    )}
                  </p>
                  <p>
                    {productSourceLabel(product)}
                    {#if productBarcode(product)}
                      · {productBarcode(product)}
                    {/if}
                    {#if isDeletedProduct(product)}
                      · Deleted
                    {/if}
                  </p>
                </div>
                <a class="secondary-action small" href={`/products/${product.id}`}>Open</a>
              </article>
            {/each}
          </div>
        {/if}
      </section>
    </section>
  {/if}
</main>
