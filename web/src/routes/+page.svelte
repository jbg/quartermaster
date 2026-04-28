<script lang="ts">
  import { browser } from '$app/environment';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import { generatedTransport } from '$lib/api';
  import { appPath } from '$lib/paths';
  import {
    batchProductId,
    buildStockUpdateRequest,
    canRestoreBatch,
    emptyInventoryState,
    eventActor,
    eventCreated,
    eventDelta,
    eventType,
    groupInventoryByLocation,
    inventoryFilterLabel,
    isDepleted,
    loadInventory,
    productBrand,
    productPreferredUnit,
    productSource,
    selectBatchAfterRefresh,
    stockCreated,
    stockDepletedAt,
    stockEditFields,
    stockExpiry,
    stockInitialQuantity,
    stockLocation,
    stockLocationId,
    stockName,
    stockOpened,
    stockUnit,
    unitChoicesForFamily,
    validateAddStockInput,
    validateStockEditInput,
    type InventoryFilterMode,
    type InventoryLocationGroup,
    type InventoryProductGroup,
    type InventoryState
  } from '$lib/inventory';
  import { sortLocations } from '$lib/locations';
  import {
    actionDone,
    emptyReminderState,
    formatReminderDate,
    formatReminderDateTime,
    loadReminders,
    optimisticAckRollback,
    optimisticAckStart,
    reminderBatchId,
    reminderBody,
    reminderExpiresOn,
    reminderFireAt,
    reminderTitle,
    reminderUrgency,
    startReminderAction,
    type ReminderState
  } from '$lib/reminders';
  import {
    reminderActionLabel,
    reminderActionStatus,
    reminderMessages
  } from '$lib/reminder-messages';
  import { barcodeLookupErrorMessage } from '$lib/products';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type Location,
    type MeResponse,
    type Product,
    type Reminder,
    type StockBatch,
    type StockEvent,
    type Unit,
    type UnitFamily
  } from '$lib/session-core';

  interface HistoryState {
    status: 'idle' | 'loading' | 'loaded' | 'error';
    items: StockEvent[];
    nextBefore: string | null;
    nextBeforeId: string | null;
    error: string | null;
  }

  const emptyHistoryState: HistoryState = {
    status: 'idle',
    items: [],
    nextBefore: null,
    nextBeforeId: null,
    error: null
  };

  let session: QuartermasterSession | null = $state(null);
  let serverUrl = $state('');
  let username = $state('');
  let password = $state('');
  let householdName = $state('');
  let timezone = $state(Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC');
  let authMode = $state<'login' | 'register'>('login');
  let me = $state<MeResponse | null>(null);
  let authError = $state<string | null>(null);
  let authBusy = $state(false);
  let authenticated = $state(false);
  let inventory = $state<InventoryState>(emptyInventoryState);
  let inventoryFilter = $state<InventoryFilterMode>('active');
  let inventorySearch = $state('');
  let reminders = $state<ReminderState>(emptyReminderState);
  let locations = $state<Location[]>([]);
  let units = $state<Unit[]>([]);
  let selectedBatchId = $state<string | null>(null);
  let selectedBatch = $state<StockBatch | null>(null);
  let history = $state<HistoryState>(emptyHistoryState);
  let consumeQuantity = $state('');
  let stockActionBusy = $state<string | null>(null);
  let stockActionError = $state<string | null>(null);
  let stockEditOpen = $state(false);
  let stockEditQuantity = $state('');
  let stockEditLocationId = $state('');
  let stockEditExpiresOn = $state('');
  let stockEditOpenedOn = $state('');
  let stockEditNote = $state('');
  let stockEditBusy = $state(false);
  let stockEditError = $state<string | null>(null);
  let highlightBatchId = $state<string | null>(null);
  let addStockOpen = $state(false);
  let productSearchQuery = $state('');
  let productSearchStatus = $state<'idle' | 'loading' | 'loaded' | 'error'>('idle');
  let productSearchResults = $state<Product[]>([]);
  let barcodeLookupValue = $state('');
  let barcodeLookupBusy = $state(false);
  let barcodeLookupError = $state<string | null>(null);
  let selectedProduct = $state<Product | null>(null);
  let manualProductName = $state('');
  let manualProductBrand = $state('');
  let manualProductFamily = $state<UnitFamily>('mass');
  let manualProductUnit = $state('g');
  let addStockQuantity = $state('');
  let addStockUnit = $state('g');
  let addStockLocationId = $state('');
  let addStockExpiresOn = $state('');
  let addStockOpenedOn = $state('');
  let addStockNote = $state('');
  let lastAddStockLocationId = $state<string | null>(null);
  let addStockBusy = $state(false);
  let addStockError = $state<string | null>(null);
  let manualProductBusy = $state(false);
  let manualProductError = $state<string | null>(null);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const restoreAvailable = $derived(canRestoreBatch(selectedBatch, history.items));
  const inventoryLocationGroups = $derived(
    sortHighlightedLocationGroups(
      groupInventoryByLocation({
        items: inventory.items,
        locations,
        filter: inventoryFilter,
        search: inventorySearch,
        highlightBatchId
      })
    )
  );
  const inventoryActiveCount = $derived(
    inventoryLocationGroups.reduce((sum, group) => sum + group.activeCount, 0)
  );
  const inventoryDepletedCount = $derived(
    inventoryLocationGroups.reduce((sum, group) => sum + group.depletedCount, 0)
  );
  const visibleProductGroupCount = $derived(
    inventoryLocationGroups.reduce((sum, group) => sum + group.productGroups.length, 0)
  );
  const addStockUnitChoices = $derived(
    selectedProduct
      ? unitChoicesForFamily(selectedProduct.family, units)
      : unitChoicesForFamily(manualProductFamily, units)
  );
  const manualProductUnitChoices = $derived(unitChoicesForFamily(manualProductFamily, units));
  const inventoryHref = $derived(appPath('/', page.url));
  const productsHref = $derived(appPath('/products', page.url));
  const settingsHref = $derived(appPath('/settings', page.url));
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
    serverUrl = created.snapshot().serverUrl;
    authenticated = true;
    void refreshMe();
  });

  async function refreshMe() {
    if (!session) {
      return;
    }
    authError = null;
    try {
      me = await session.me();
      if (currentHousehold(me)) {
        await refreshWorkspace(selectedBatchId);
      } else {
        clearHouseholdState();
      }
    } catch {
      me = null;
      authenticated = false;
      clearHouseholdState();
      authError = 'Sign in again to continue.';
    }
  }

  async function refreshWorkspace(preferredBatchId: string | null = selectedBatchId) {
    await refreshUnits();
    await refreshLocations();
    await refreshInventory(preferredBatchId);
    await refreshReminders();
  }

  async function refreshUnits() {
    if (!session) {
      return;
    }
    try {
      units = await session.unitsList();
    } catch {
      units = [];
    }
  }

  async function refreshLocations() {
    if (!session) {
      return;
    }
    try {
      const rows = await session.locationsList();
      locations = sortLocations(rows);
      if (!addStockLocationId && locations[0]) {
        addStockLocationId = preferredAddStockLocationId();
      }
    } catch {
      locations = [];
    }
  }

  async function refreshInventory(preferredBatchId: string | null = selectedBatchId) {
    if (!session) {
      return;
    }
    inventory = { status: 'loading', items: inventory.items, error: null };
    const nextInventory = await loadInventory(session);
    inventory = nextInventory;
    if (nextInventory.status !== 'loaded') {
      selectedBatch = null;
      selectedBatchId = null;
      history = emptyHistoryState;
      return;
    }
    const nextSelection = selectBatchAfterRefresh(nextInventory.items, preferredBatchId);
    selectedBatch = nextSelection;
    selectedBatchId = nextSelection?.id ?? null;
    if (nextSelection) {
      await refreshBatchDetail(nextSelection.id);
    } else {
      history = emptyHistoryState;
    }
  }

  async function refreshBatchDetail(id: string) {
    if (!session) {
      return;
    }
    history = { ...history, status: 'loading', error: null };
    try {
      const [batch, events] = await Promise.all([
        session.stockGet(id),
        session.stockListBatchEvents(id, { limit: 25 })
      ]);
      selectedBatch = batch;
      selectedBatchId = batch.id;
      history = {
        status: 'loaded',
        items: events.items ?? [],
        nextBefore: events.next_before ?? events.nextBefore ?? null,
        nextBeforeId: events.next_before_id ?? events.nextBeforeId ?? null,
        error: null
      };
    } catch {
      history = {
        status: 'error',
        items: [],
        nextBefore: null,
        nextBeforeId: null,
        error: 'Stock history could not be loaded.'
      };
    }
  }

  async function loadMoreHistory() {
    if (!session || !selectedBatchId || !history.nextBefore || !history.nextBeforeId) {
      return;
    }
    const previous = history;
    history = { ...history, status: 'loading', error: null };
    try {
      const page = await session.stockListBatchEvents(selectedBatchId, {
        before_created_at: previous.nextBefore,
        before_id: previous.nextBeforeId,
        limit: 25
      });
      history = {
        status: 'loaded',
        items: [...previous.items, ...(page.items ?? [])],
        nextBefore: page.next_before ?? page.nextBefore ?? null,
        nextBeforeId: page.next_before_id ?? page.nextBeforeId ?? null,
        error: null
      };
    } catch {
      history = { ...previous, status: 'error', error: 'More history could not be loaded.' };
    }
  }

  async function refreshReminders({ preserveItems = false } = {}) {
    if (!session) {
      return;
    }
    const previousItems = reminders.items;
    reminders = {
      ...reminders,
      status: 'loading',
      error: null,
      items: preserveItems ? reminders.items : []
    };
    reminders = await loadReminders(
      session,
      reminders.actionIds,
      reminders.actionKinds,
      preserveItems ? previousItems : []
    );
  }

  async function submitAuth() {
    if (!session) {
      return;
    }
    authBusy = true;
    authError = null;
    session.setServerUrl(serverUrl);
    try {
      if (authMode === 'login') {
        await session.login(username, password);
      } else {
        await session.createOnboardingHousehold(username, password, householdName, timezone);
      }
      authenticated = true;
      await refreshMe();
    } catch {
      authError = authMode === 'login' ? 'Login failed.' : 'Registration failed.';
    } finally {
      authBusy = false;
    }
  }

  async function logout() {
    if (!session) {
      return;
    }
    await session.logout();
    authenticated = false;
    me = null;
    clearHouseholdState();
  }

  async function switchHousehold(id: string) {
    if (!session) {
      return;
    }
    authError = null;
    try {
      me = await session.switchHousehold(id);
      await refreshWorkspace(null);
    } catch {
      authError = 'Household could not be switched.';
    }
  }

  async function selectBatch(batch: StockBatch) {
    selectedBatch = batch;
    selectedBatchId = batch.id;
    stockActionError = null;
    closeStockEdit();
    await refreshBatchDetail(batch.id);
  }

  async function selectProductGroup(group: InventoryProductGroup) {
    await selectBatch(group.bestBatch);
  }

  async function openReminder(reminder: Reminder) {
    if (!session) {
      return;
    }
    const id = reminder.id;
    reminders = startReminderAction(reminders, id, 'open');
    try {
      await session.remindersOpen(id);
      const batchId = reminderBatchId(reminder);
      highlightBatchId = batchId;
      inventoryFilter = 'all';
      inventorySearch = '';
      await refreshInventory(batchId);
      await refreshReminders({ preserveItems: true });
    } catch {
      reminders = { ...reminders, error: reminderMessages.openError };
    } finally {
      reminders = actionDone(reminders, id);
    }
  }

  async function ackReminder(reminder: Reminder) {
    if (!session) {
      return;
    }
    reminders = optimisticAckStart(reminders, reminder.id);
    try {
      await session.remindersAck(reminder.id);
      reminders = actionDone(reminders, reminder.id);
      await refreshReminders({ preserveItems: true });
    } catch {
      reminders = optimisticAckRollback(reminders, reminder, reminderMessages.ackError);
    }
  }

  async function consumeSelected() {
    if (!session || !selectedBatch) {
      return;
    }
    const quantity = consumeQuantity.trim();
    if (!quantity || Number(quantity) <= 0) {
      stockActionError = 'Enter a positive quantity to consume.';
      return;
    }
    stockActionBusy = 'consume';
    stockActionError = null;
    try {
      await session.stockConsume({
        product_id: batchProductId(selectedBatch),
        location_id: stockLocationId(selectedBatch),
        quantity,
        unit: stockUnit(selectedBatch)
      });
      consumeQuantity = '';
      await refreshWorkspace(selectedBatch.id);
    } catch {
      stockActionError = 'Stock could not be consumed.';
    } finally {
      stockActionBusy = null;
    }
  }

  function hydrateStockEdit(batch: StockBatch) {
    const fields = stockEditFields(batch);
    stockEditQuantity = fields.quantity;
    stockEditLocationId = fields.locationId;
    stockEditExpiresOn = fields.expiresOn;
    stockEditOpenedOn = fields.openedOn;
    stockEditNote = fields.note;
    stockEditError = null;
  }

  function openStockEdit() {
    if (!selectedBatch || isDepleted(selectedBatch)) {
      return;
    }
    hydrateStockEdit(selectedBatch);
    stockEditOpen = true;
  }

  function closeStockEdit() {
    stockEditOpen = false;
    stockEditBusy = false;
    stockEditError = null;
  }

  async function submitStockEdit() {
    if (!session || !selectedBatch || isDepleted(selectedBatch)) {
      return;
    }
    const fields = {
      quantity: stockEditQuantity,
      locationId: stockEditLocationId,
      expiresOn: stockEditExpiresOn,
      openedOn: stockEditOpenedOn,
      note: stockEditNote
    };
    const validationError = validateStockEditInput(fields);
    if (validationError) {
      stockEditError = validationError;
      return;
    }
    stockEditBusy = true;
    stockEditError = null;
    try {
      const updated = await session.stockUpdate(
        selectedBatch.id,
        buildStockUpdateRequest(selectedBatch, fields)
      );
      stockEditOpen = false;
      highlightBatchId = updated.id;
      await refreshWorkspace(updated.id);
    } catch {
      stockEditError = 'Stock could not be updated.';
    } finally {
      stockEditBusy = false;
    }
  }

  async function discardSelected() {
    if (!session || !selectedBatch) {
      return;
    }
    stockActionBusy = 'discard';
    stockActionError = null;
    closeStockEdit();
    try {
      await session.stockDelete(selectedBatch.id);
      await refreshWorkspace(selectedBatch.id);
    } catch {
      stockActionError = 'Stock could not be discarded.';
    } finally {
      stockActionBusy = null;
    }
  }

  async function restoreSelected() {
    if (!session || !selectedBatch) {
      return;
    }
    stockActionBusy = 'restore';
    stockActionError = null;
    try {
      await session.stockRestore(selectedBatch.id);
      await refreshWorkspace(selectedBatch.id);
    } catch {
      stockActionError = 'Stock could not be restored.';
    } finally {
      stockActionBusy = null;
    }
  }

  function openAddStock() {
    addStockOpen = true;
    addStockError = null;
    manualProductError = null;
    if (!addStockLocationId && locations[0]) {
      addStockLocationId = preferredAddStockLocationId();
    }
  }

  function closeAddStock() {
    addStockOpen = false;
    resetAddStockForm();
  }

  function resetAddStockForm() {
    productSearchQuery = '';
    productSearchStatus = 'idle';
    productSearchResults = [];
    barcodeLookupValue = '';
    barcodeLookupBusy = false;
    barcodeLookupError = null;
    selectedProduct = null;
    manualProductName = '';
    manualProductBrand = '';
    manualProductFamily = 'mass';
    manualProductUnit = unitChoicesForFamily('mass', units)[0];
    addStockQuantity = '';
    addStockUnit = unitChoicesForFamily('mass', units)[0];
    addStockLocationId = preferredAddStockLocationId();
    addStockExpiresOn = '';
    addStockOpenedOn = '';
    addStockNote = '';
    addStockBusy = false;
    addStockError = null;
    manualProductBusy = false;
    manualProductError = null;
  }

  async function searchProducts() {
    if (!session) {
      return;
    }
    const query = productSearchQuery.trim();
    selectedProduct = null;
    addStockError = null;
    if (!query) {
      productSearchStatus = 'idle';
      productSearchResults = [];
      return;
    }
    productSearchStatus = 'loading';
    try {
      const response = await session.productSearch({ q: query, limit: 12 });
      productSearchResults = response.items ?? [];
      productSearchStatus = 'loaded';
    } catch {
      productSearchResults = [];
      productSearchStatus = 'error';
    }
  }

  function chooseProduct(product: Product) {
    selectedProduct = product;
    addStockUnit = productPreferredUnit(product, units);
    addStockError = null;
    barcodeLookupError = null;
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
    addStockError = null;
    try {
      const response = await session.productByBarcode(barcode);
      const product = response.product;
      chooseProduct(product);
      productSearchResults = [
        product,
        ...productSearchResults.filter((item) => item.id !== product.id)
      ];
      productSearchStatus = 'loaded';
      productSearchQuery = product.name;
    } catch (err) {
      barcodeLookupError = barcodeLookupErrorMessage(err);
    } finally {
      barcodeLookupBusy = false;
    }
  }

  function setManualProductFamily(family: string) {
    if (family !== 'mass' && family !== 'volume' && family !== 'count') {
      return;
    }
    manualProductFamily = family;
    manualProductUnit = unitChoicesForFamily(family, units)[0];
  }

  async function createManualProduct() {
    if (!session) {
      return;
    }
    const name = manualProductName.trim();
    if (!name) {
      manualProductError = 'Enter a product name.';
      return;
    }
    manualProductBusy = true;
    manualProductError = null;
    try {
      const product = await session.productCreate({
        name,
        brand: manualProductBrand.trim() || null,
        family: manualProductFamily,
        preferred_unit: manualProductUnit,
        barcode: null,
        image_url: null
      });
      chooseProduct(product);
      productSearchResults = [
        product,
        ...productSearchResults.filter((item) => item.id !== product.id)
      ];
      productSearchStatus = 'loaded';
      productSearchQuery = product.name;
      manualProductName = '';
      manualProductBrand = '';
    } catch {
      manualProductError = 'Product could not be created.';
    } finally {
      manualProductBusy = false;
    }
  }

  async function submitAddStock() {
    if (!session) {
      return;
    }
    const validationError = validateAddStockInput({
      product: selectedProduct,
      quantity: addStockQuantity,
      locationId: addStockLocationId
    });
    if (validationError) {
      addStockError = validationError;
      return;
    }
    addStockBusy = true;
    addStockError = null;
    try {
      const createdLocationId = addStockLocationId;
      const created = await session.stockCreate({
        product_id: selectedProduct!.id,
        location_id: createdLocationId,
        quantity: addStockQuantity.trim(),
        unit: addStockUnit,
        expires_on: addStockExpiresOn || null,
        opened_on: addStockOpenedOn || null,
        note: addStockNote.trim() || null
      });
      lastAddStockLocationId = createdLocationId;
      inventoryFilter = 'active';
      inventorySearch = '';
      addStockOpen = false;
      resetAddStockForm();
      highlightBatchId = created.id;
      await refreshWorkspace(created.id);
    } catch {
      addStockError = 'Stock could not be added.';
    } finally {
      addStockBusy = false;
    }
  }

  function clearHouseholdState() {
    inventory = emptyInventoryState;
    reminders = emptyReminderState;
    locations = [];
    units = [];
    selectedBatch = null;
    selectedBatchId = null;
    history = emptyHistoryState;
    consumeQuantity = '';
    stockActionBusy = null;
    stockActionError = null;
    closeStockEdit();
    highlightBatchId = null;
    lastAddStockLocationId = null;
    resetAddStockForm();
    addStockOpen = false;
  }

  function formatDateTime(value: string | undefined | null): string {
    if (!value) {
      return '';
    }
    const parsed = Date.parse(value);
    return Number.isNaN(parsed) ? value : new Date(parsed).toLocaleString();
  }

  function displayLocation(batch: StockBatch): string {
    return stockLocation(batch);
  }

  function preferredAddStockLocationId(): string {
    if (
      lastAddStockLocationId &&
      locations.some((location) => location.id === lastAddStockLocationId)
    ) {
      return lastAddStockLocationId;
    }
    return locations[0]?.id ?? '';
  }

  function sortHighlightedLocationGroups(
    groups: InventoryLocationGroup[]
  ): InventoryLocationGroup[] {
    const highlighted = highlightBatchId;
    if (!highlighted) {
      return groups;
    }
    return [...groups].sort((left, right) => {
      const leftHasHighlight = locationGroupHasBatch(left, highlighted);
      const rightHasHighlight = locationGroupHasBatch(right, highlighted);
      if (leftHasHighlight !== rightHasHighlight) {
        return leftHasHighlight ? -1 : 1;
      }
      return 0;
    });
  }

  function locationGroupHasBatch(group: InventoryLocationGroup, batchId: string): boolean {
    return group.productGroups.some((productGroup) =>
      productGroup.visibleBatches.some((batch) => batch.id === batchId)
    );
  }

  function productGroupQuantity(group: InventoryProductGroup): string {
    if (group.totalQuantity && group.totalUnit) {
      return `${group.totalQuantity} ${group.totalUnit}`;
    }
    return `${group.visibleBatches.length} ${group.visibleBatches.length === 1 ? 'batch' : 'batches'}`;
  }

  function productGroupMeta(group: InventoryProductGroup): string {
    const parts = [
      group.earliestExpiry ? `Earliest ${group.earliestExpiry}` : 'No expiry date',
      `${group.visibleBatches.length} ${group.visibleBatches.length === 1 ? 'batch' : 'batches'}`
    ];
    if (group.depletedCount > 0 && group.activeCount === 0) {
      parts.push('depleted history');
    } else if (group.depletedCount > 0) {
      parts.push(`${group.depletedCount} depleted`);
    }
    return parts.join(' - ');
  }
</script>

<svelte:head>
  <title>Quartermaster</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div class="brand-heading">
      <img class="brand-mark" src={brandMarkSrc} alt="" />
      <div>
        <p class="eyebrow">Quartermaster</p>
        <h1>Kitchen inventory</h1>
      </div>
    </div>
    {#if authenticated}
      <div class="heading-actions">
        <a class="secondary-action" href={productsHref}>Products</a>
        <a class="secondary-action" href={settingsHref}>Settings</a>
        <button class="ghost-button" type="button" onclick={logout}>Log out</button>
      </div>
    {/if}
  </header>

  {#if !authenticated}
    <section class="auth-layout">
      <form
        class="panel auth-panel"
        onsubmit={(event) => {
          event.preventDefault();
          void submitAuth();
        }}
      >
        <div class="segmented">
          <button
            class:active={authMode === 'login'}
            type="button"
            onclick={() => (authMode = 'login')}>Login</button
          >
          <button
            class:active={authMode === 'register'}
            type="button"
            onclick={() => (authMode = 'register')}>Register</button
          >
        </div>

        <details class="advanced-auth">
          <summary>Change server</summary>
          <label>
            Server URL
            <input bind:value={serverUrl} placeholder="http://localhost:8080" autocomplete="url" />
          </label>
        </details>
        <label>
          Username
          <input bind:value={username} autocomplete="username" required />
        </label>
        <label>
          Password
          <input
            bind:value={password}
            type="password"
            autocomplete={authMode === 'login' ? 'current-password' : 'new-password'}
            required
            minlength="8"
          />
        </label>

        {#if authMode === 'register'}
          <label>
            Household name
            <input bind:value={householdName} required />
          </label>
          <label>
            Timezone
            <input bind:value={timezone} required />
          </label>
        {/if}

        {#if authError}
          <p class="error-text">{authError}</p>
        {/if}

        <button
          class="primary-action"
          type="submit"
          disabled={authBusy ||
            !username ||
            password.length < 8 ||
            (authMode === 'register' && (!householdName || !timezone))}
        >
          {authBusy ? 'Working...' : authMode === 'login' ? 'Log in' : 'Create account'}
        </button>
      </form>
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p>Switch to an existing household from this account.</p>
      {#if households.length > 0}
        <div class="household-list">
          {#each households as household}
            <button type="button" onclick={() => switchHousehold(household.id)}
              >{household.name}</button
            >
          {/each}
        </div>
      {:else}
        <p class="muted">Create or join a household from a native app for now.</p>
      {/if}
    </section>
  {:else if me && activeHousehold}
    {#if addStockOpen}
      <section class="panel add-stock-panel" aria-labelledby="add-stock-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">New batch</p>
            <h2 id="add-stock-heading">Add stock</h2>
          </div>
          <button class="ghost-button" type="button" onclick={closeAddStock}>Close</button>
        </div>

        <div class="add-stock-grid">
          <section class="add-stock-column" aria-labelledby="product-picker-heading">
            <div class="section-heading compact">
              <div>
                <p class="eyebrow">Product</p>
                <h3 id="product-picker-heading">Find existing</h3>
              </div>
            </div>
            <form
              class="inline-form"
              onsubmit={(event) => {
                event.preventDefault();
                void searchProducts();
              }}
            >
              <label>
                Product search
                <input bind:value={productSearchQuery} placeholder="Rice, pasta, beans..." />
              </label>
              <button
                class="secondary-action"
                type="submit"
                disabled={!productSearchQuery.trim() || productSearchStatus === 'loading'}
              >
                {productSearchStatus === 'loading' ? 'Searching...' : 'Search'}
              </button>
            </form>

            {#if productSearchStatus === 'error'}
              <p class="error-text">Products could not be searched.</p>
            {:else if productSearchStatus === 'loaded' && productSearchResults.length === 0}
              <p class="muted">No products match this search.</p>
            {:else if productSearchResults.length > 0}
              <div class="product-results">
                {#each productSearchResults as product}
                  <button
                    class:active={selectedProduct?.id === product.id}
                    class="product-result"
                    type="button"
                    onclick={() => chooseProduct(product)}
                  >
                    <div>
                      <h4>{product.name}</h4>
                      <p>
                        {productBrand(product) || 'No brand'} - {product.family} - {productPreferredUnit(
                          product,
                          units
                        )}
                      </p>
                    </div>
                    <span>{productSource(product)}</span>
                  </button>
                {/each}
              </div>
            {/if}

            <div class="barcode-product" data-testid="inventory-barcode-lookup">
              <div class="section-heading compact">
                <div>
                  <p class="eyebrow">Barcode</p>
                  <h3>Look up product</h3>
                </div>
              </div>
              <form
                class="inline-form"
                onsubmit={(event) => {
                  event.preventDefault();
                  void lookupBarcodeProduct();
                }}
              >
                <label>
                  Barcode
                  <input
                    bind:value={barcodeLookupValue}
                    data-testid="inventory-barcode-lookup-input"
                    inputmode="numeric"
                    placeholder="EAN or UPC"
                  />
                </label>
                <button
                  class="secondary-action"
                  type="submit"
                  data-testid="inventory-barcode-lookup-submit"
                  disabled={!barcodeLookupValue.trim() || barcodeLookupBusy}
                >
                  {barcodeLookupBusy ? 'Looking up...' : 'Look up'}
                </button>
              </form>
              {#if barcodeLookupError}
                <p class="error-text">{barcodeLookupError}</p>
              {/if}
            </div>

            <div class="manual-product">
              <div class="section-heading compact">
                <div>
                  <p class="eyebrow">Manual</p>
                  <h3>Create product</h3>
                </div>
              </div>
              <form
                class="manual-product-form"
                onsubmit={(event) => {
                  event.preventDefault();
                  void createManualProduct();
                }}
              >
                <label>
                  Product name
                  <input bind:value={manualProductName} required />
                </label>
                <label>
                  Brand
                  <input bind:value={manualProductBrand} />
                </label>
                <label>
                  Product family
                  <select
                    value={manualProductFamily}
                    onchange={(event) => setManualProductFamily(event.currentTarget.value)}
                  >
                    <option value="mass">Mass</option>
                    <option value="volume">Volume</option>
                    <option value="count">Count</option>
                  </select>
                </label>
                <label>
                  Preferred unit
                  <select bind:value={manualProductUnit}>
                    {#each manualProductUnitChoices as unit}
                      <option value={unit}>{unit}</option>
                    {/each}
                  </select>
                </label>
                <button
                  class="secondary-action"
                  type="submit"
                  disabled={manualProductBusy || !manualProductName.trim()}
                >
                  {manualProductBusy ? 'Creating...' : 'Create product'}
                </button>
                {#if manualProductError}
                  <p class="error-text">{manualProductError}</p>
                {/if}
              </form>
            </div>
          </section>

          <form
            class="add-stock-column stock-create-form"
            onsubmit={(event) => {
              event.preventDefault();
              void submitAddStock();
            }}
          >
            <div class="section-heading compact">
              <div>
                <p class="eyebrow">Batch</p>
                <h3>Stock details</h3>
              </div>
            </div>

            <div class="selected-product" data-testid="selected-product">
              <span>Selected product</span>
              <strong>{selectedProduct ? selectedProduct.name : 'None selected'}</strong>
            </div>

            <label>
              Stock quantity
              <input bind:value={addStockQuantity} inputmode="decimal" placeholder="1" />
            </label>
            <label>
              Unit
              <select bind:value={addStockUnit} disabled={!selectedProduct}>
                {#each addStockUnitChoices as unit}
                  <option value={unit}>{unit}</option>
                {/each}
              </select>
            </label>
            <label>
              Location
              <select bind:value={addStockLocationId}>
                {#each locations as location}
                  <option value={location.id}>{location.name}</option>
                {/each}
              </select>
            </label>
            {#if locations.length === 0}
              <p class="error-text">No locations are available for this household.</p>
            {/if}
            <label>
              Expiry date
              <input bind:value={addStockExpiresOn} type="date" />
            </label>
            <label>
              Opened date
              <input bind:value={addStockOpenedOn} type="date" />
            </label>
            <label>
              Note
              <input bind:value={addStockNote} />
            </label>
            {#if addStockError}
              <p class="error-text">{addStockError}</p>
            {/if}
            <button
              class="primary-action"
              type="submit"
              disabled={addStockBusy || locations.length === 0}
            >
              {addStockBusy ? 'Adding...' : 'Add stock'}
            </button>
          </form>
        </div>
      </section>
    {/if}

    <section class="workspace">
      <aside class="sidebar">
        <div>
          <p class="eyebrow">Current household</p>
          <h2>{activeHousehold.name}</h2>
        </div>
        {#if households.length > 1}
          <label>
            Switch household
            <select
              onchange={(event) => switchHousehold(event.currentTarget.value)}
              value={activeHousehold.id}
            >
              {#each households as household}
                <option value={household.id}>{household.name}</option>
              {/each}
            </select>
          </label>
        {/if}
        <button
          class="secondary-action"
          type="button"
          onclick={() => refreshWorkspace(selectedBatchId)}>Refresh</button
        >

        <section class="inbox-region" aria-labelledby="reminder-heading">
          <div class="section-heading compact">
            <div>
              <p class="eyebrow">{reminderMessages.headingEyebrow}</p>
              <h2 id="reminder-heading">{reminderMessages.headingTitle}</h2>
            </div>
            <span>{reminders.items.length}</span>
          </div>

          {#if reminders.status === 'loading' && reminders.items.length === 0}
            <p class="muted">{reminderMessages.loading}</p>
          {:else if reminders.status === 'error' && reminders.items.length === 0}
            <p class="error-text">{reminders.error}</p>
          {:else if reminders.items.length === 0}
            <p class="muted">{reminderMessages.empty}</p>
          {:else}
            {#if reminders.status === 'loading'}
              <p class="muted">{reminderMessages.refreshing}</p>
            {:else if reminders.status === 'error'}
              <p class="error-text">{reminders.error}</p>
            {/if}
            <div class="reminder-list">
              {#each reminders.items as reminder}
                <article class="reminder-row" data-testid={`reminder-row-${reminder.id}`}>
                  <div>
                    <h3>{reminderTitle(reminder)}</h3>
                    <p>{reminderBody(reminder)}</p>
                    {#if reminderExpiresOn(reminder)}
                      <span>{reminderUrgency(reminder)}</span>
                      <span>
                        {reminderMessages.expiryDateLabel}
                        {formatReminderDate(reminderExpiresOn(reminder))}
                      </span>
                    {/if}
                    <span>
                      {reminderMessages.householdTimeLabel}
                      {formatReminderDateTime(reminderFireAt(reminder))}
                    </span>
                    {#if reminderActionStatus(reminders.actionKinds[reminder.id])}
                      <span class="inline-status">
                        {reminderActionStatus(reminders.actionKinds[reminder.id])}
                      </span>
                    {/if}
                  </div>
                  <div class="row-actions">
                    <button
                      class="secondary-action small"
                      type="button"
                      data-testid={`reminder-open-${reminder.id}`}
                      disabled={reminders.actionIds.has(reminder.id)}
                      onclick={() => openReminder(reminder)}
                      >{reminderActionLabel(reminders.actionKinds[reminder.id], 'open')}</button
                    >
                    <button
                      class="ghost-button small"
                      type="button"
                      data-testid={`reminder-ack-${reminder.id}`}
                      disabled={reminders.actionIds.has(reminder.id)}
                      onclick={() => ackReminder(reminder)}
                      >{reminderActionLabel(reminders.actionKinds[reminder.id], 'ack')}</button
                    >
                  </div>
                </article>
              {/each}
            </div>
          {/if}
        </section>
      </aside>

      <section class="inventory-region">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Inventory</p>
            <h2>Batches</h2>
          </div>
          <div class="heading-actions">
            <span>{inventoryActiveCount} active - {inventoryDepletedCount} depleted</span>
            <button class="primary-action small" type="button" onclick={openAddStock}
              >Add stock</button
            >
          </div>
        </div>

        <div class="inventory-controls">
          <label>
            Search inventory
            <input
              bind:value={inventorySearch}
              data-testid="inventory-search-input"
              placeholder="Product, location, note..."
            />
          </label>
          <label>
            Show
            <select bind:value={inventoryFilter} data-testid="inventory-filter-select">
              <option value="active">Active</option>
              <option value="expiring_soon">Expiring soon</option>
              <option value="expired">Expired</option>
              <option value="depleted">Depleted</option>
              <option value="all">All</option>
            </select>
          </label>
        </div>

        {#if inventory.status === 'loading'}
          <p class="muted">Loading inventory...</p>
        {:else if inventory.status === 'error'}
          <p class="error-text">{inventory.error}</p>
        {:else if inventory.items.length === 0}
          <p class="muted">No stock recorded.</p>
        {:else if visibleProductGroupCount === 0}
          <p class="muted">
            No {inventoryFilterLabel(inventoryFilter).toLowerCase()} stock matches the current search.
          </p>
        {:else}
          <div class="location-inventory-list">
            {#each inventoryLocationGroups as locationGroup}
              <section
                class="location-inventory-group"
                data-testid={`inventory-location-${locationGroup.location.name}`}
              >
                <div class="subsection-heading">
                  <div>
                    <h3>{locationGroup.location.name}</h3>
                    <p>
                      {locationGroup.activeCount} active - {locationGroup.depletedCount} depleted
                    </p>
                  </div>
                  <span>{locationGroup.productGroups.length}</span>
                </div>
                {#if locationGroup.productGroups.length === 0}
                  <p class="muted location-empty">{locationGroup.emptyMessage}</p>
                {:else}
                  {#each locationGroup.productGroups as productGroup}
                    <button
                      class:active={productGroup.visibleBatches.some(
                        (batch) => selectedBatchId === batch.id
                      )}
                      class:highlight={productGroup.visibleBatches.some(
                        (batch) => highlightBatchId === batch.id
                      )}
                      class="stock-row product-group-row"
                      type="button"
                      onclick={() => selectProductGroup(productGroup)}
                    >
                      <div>
                        <h3>{productGroup.productName}</h3>
                        <p>
                          {productGroup.productBrand
                            ? `${productGroup.productBrand} - `
                            : ''}{productGroupMeta(productGroup)}
                        </p>
                      </div>
                      <div class="product-group-summary">
                        <strong>{productGroupQuantity(productGroup)}</strong>
                        <span>Open</span>
                      </div>
                    </button>
                  {/each}
                {/if}
              </section>
            {/each}
          </div>
        {/if}
      </section>

      <section class="detail-region">
        {#if selectedBatch}
          <div class="section-heading">
            <div>
              <p class="eyebrow" data-testid="detail-status">
                {isDepleted(selectedBatch) ? 'Depleted' : 'In stock'}
              </p>
              <h2>{stockName(selectedBatch)}</h2>
            </div>
            <div class="heading-actions">
              {#if !isDepleted(selectedBatch)}
                <button
                  class="secondary-action small"
                  type="button"
                  disabled={stockActionBusy !== null || stockEditBusy}
                  onclick={openStockEdit}>{stockEditOpen ? 'Reset edit' : 'Edit'}</button
                >
              {/if}
              <strong data-testid="detail-quantity"
                >{selectedBatch.quantity ?? '?'} {stockUnit(selectedBatch)}</strong
              >
            </div>
          </div>

          <dl class="detail-grid">
            <div>
              <dt>Location</dt>
              <dd>{displayLocation(selectedBatch)}</dd>
            </div>
            <div>
              <dt>Expires</dt>
              <dd>{stockExpiry(selectedBatch)}</dd>
            </div>
            <div>
              <dt>Opened</dt>
              <dd>{stockOpened(selectedBatch)}</dd>
            </div>
            <div>
              <dt>Initial quantity</dt>
              <dd>{stockInitialQuantity(selectedBatch) || 'Unknown'} {stockUnit(selectedBatch)}</dd>
            </div>
            <div>
              <dt>Created</dt>
              <dd>{formatDateTime(stockCreated(selectedBatch)) || 'Unknown'}</dd>
            </div>
            {#if isDepleted(selectedBatch)}
              <div>
                <dt>Depleted</dt>
                <dd>{formatDateTime(stockDepletedAt(selectedBatch)) || 'Unknown'}</dd>
              </div>
            {/if}
            <div>
              <dt>Note</dt>
              <dd>{selectedBatch.note || 'None'}</dd>
            </div>
          </dl>

          {#if stockEditOpen && !isDepleted(selectedBatch)}
            <form
              class="action-panel stock-edit-form"
              onsubmit={(event) => {
                event.preventDefault();
                void submitStockEdit();
              }}
            >
              <div class="section-heading compact">
                <div>
                  <p class="eyebrow">Correction</p>
                  <h2>Edit batch</h2>
                </div>
              </div>
              <label>
                Stock quantity
                <input bind:value={stockEditQuantity} inputmode="decimal" />
              </label>
              <label>
                Unit
                <input value={stockUnit(selectedBatch)} readonly />
              </label>
              <label>
                Location
                <select bind:value={stockEditLocationId}>
                  {#each locations as location}
                    <option value={location.id}>{location.name}</option>
                  {/each}
                </select>
              </label>
              <label>
                Expiry date
                <input bind:value={stockEditExpiresOn} type="date" />
              </label>
              <label>
                Opened date
                <input bind:value={stockEditOpenedOn} type="date" />
              </label>
              <label>
                Note
                <input bind:value={stockEditNote} />
              </label>
              {#if stockEditError}
                <p class="error-text">{stockEditError}</p>
              {/if}
              <div class="stock-actions">
                <button
                  class="primary-action"
                  type="submit"
                  disabled={stockEditBusy || stockActionBusy !== null || locations.length === 0}
                  >{stockEditBusy ? 'Saving...' : 'Save changes'}</button
                >
                <button
                  class="ghost-button"
                  type="button"
                  disabled={stockEditBusy}
                  onclick={closeStockEdit}>Cancel</button
                >
              </div>
            </form>
          {/if}

          <form
            class="action-panel"
            onsubmit={(event) => {
              event.preventDefault();
              void consumeSelected();
            }}
          >
            <label>
              Consume quantity
              <input
                bind:value={consumeQuantity}
                inputmode="decimal"
                placeholder={`Amount in ${stockUnit(selectedBatch)}`}
                disabled={isDepleted(selectedBatch) || stockEditBusy}
              />
            </label>
            <div class="stock-actions">
              <button
                class="primary-action"
                type="submit"
                disabled={stockActionBusy !== null || stockEditBusy || isDepleted(selectedBatch)}
                >Consume</button
              >
              <button
                class="secondary-action"
                type="button"
                disabled={stockActionBusy !== null || stockEditBusy || isDepleted(selectedBatch)}
                onclick={discardSelected}>Discard</button
              >
              {#if restoreAvailable}
                <button
                  class="secondary-action"
                  type="button"
                  disabled={stockActionBusy !== null || stockEditBusy}
                  onclick={restoreSelected}>Restore</button
                >
              {/if}
            </div>
            {#if stockActionError}
              <p class="error-text">{stockActionError}</p>
            {/if}
          </form>

          <section class="history-region" aria-labelledby="history-heading">
            <div class="section-heading compact">
              <div>
                <p class="eyebrow">Ledger</p>
                <h2 id="history-heading">History</h2>
              </div>
            </div>
            {#if history.status === 'loading' && history.items.length === 0}
              <p class="muted">Loading history...</p>
            {:else if history.status === 'error' && history.items.length === 0}
              <p class="error-text">{history.error}</p>
            {:else if history.items.length === 0}
              <p class="muted">No history for this batch yet.</p>
            {:else}
              <div class="history-list">
                {#each history.items as event}
                  <article class="history-row">
                    <div>
                      <h3>{eventType(event)}</h3>
                      <p>{eventActor(event)} - {formatDateTime(eventCreated(event))}</p>
                      {#if event.note}
                        <p>{event.note}</p>
                      {/if}
                    </div>
                    <strong>{eventDelta(event)} {event.unit}</strong>
                  </article>
                {/each}
              </div>
              {#if history.nextBefore && history.nextBeforeId}
                <button
                  class="secondary-action"
                  type="button"
                  disabled={history.status === 'loading'}
                  onclick={loadMoreHistory}>Load more</button
                >
              {/if}
              {#if history.error}
                <p class="error-text">{history.error}</p>
              {/if}
            {/if}
          </section>
        {:else}
          <div class="empty-state">
            <h2>Select a batch</h2>
            <p class="muted">Choose stock from the inventory list to see details and actions.</p>
          </div>
        {/if}
      </section>
    </section>
  {:else}
    <section class="panel empty-state">
      <p class="muted">Loading account...</p>
    </section>
  {/if}
</main>
