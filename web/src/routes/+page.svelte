<script lang="ts">
  import { browser } from '$app/environment';
  import { generatedTransport } from '$lib/api';
  import {
    batchProductId,
    canRestoreBatch,
    emptyInventoryState,
    eventActor,
    eventCreated,
    eventDelta,
    eventType,
    isDepleted,
    loadInventory,
    selectBatchAfterRefresh,
    stockCreated,
    stockExpiry,
    stockInitialQuantity,
    stockLocation,
    stockLocationId,
    stockName,
    stockOpened,
    stockUnit,
    type InventoryState
  } from '$lib/inventory';
  import {
    actionDone,
    emptyReminderState,
    loadReminders,
    optimisticAckRollback,
    optimisticAckStart,
    reminderBatchId,
    reminderFireAt,
    type ReminderState
  } from '$lib/reminders';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type Location,
    type MeResponse,
    type Reminder,
    type StockBatch,
    type StockEvent
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
  let email = $state('');
  let inviteCode = $state('');
  let authMode = $state<'login' | 'register'>('login');
  let me = $state<MeResponse | null>(null);
  let authError = $state<string | null>(null);
  let authBusy = $state(false);
  let authenticated = $state(false);
  let inventory = $state<InventoryState>(emptyInventoryState);
  let reminders = $state<ReminderState>(emptyReminderState);
  let locationNames = $state<Record<string, string>>({});
  let selectedBatchId = $state<string | null>(null);
  let selectedBatch = $state<StockBatch | null>(null);
  let history = $state<HistoryState>(emptyHistoryState);
  let consumeQuantity = $state('');
  let stockActionBusy = $state<string | null>(null);
  let stockActionError = $state<string | null>(null);
  let highlightBatchId = $state<string | null>(null);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const restoreAvailable = $derived(canRestoreBatch(selectedBatch, history.items));

  $effect(() => {
    if (!browser) {
      return;
    }
    const created = new QuartermasterSession(
      createBrowserSessionStorage(window.localStorage, window.location.origin),
      generatedTransport()
    );
    session = created;
    serverUrl = created.snapshot().serverUrl || window.location.origin;
    if (created.snapshot().accessToken) {
      authenticated = true;
      void refreshMe();
    }
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
      clearHouseholdState();
      authError = 'Sign in again to continue.';
    }
  }

  async function refreshWorkspace(preferredBatchId: string | null = selectedBatchId) {
    await refreshLocations();
    await refreshInventory(preferredBatchId);
    await refreshReminders();
  }

  async function refreshLocations() {
    if (!session) {
      return;
    }
    try {
      const rows = await session.locationsList();
      locationNames = Object.fromEntries(rows.map((location: Location) => [location.id, location.name]));
    } catch {
      locationNames = {};
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

  async function refreshReminders() {
    if (!session) {
      return;
    }
    reminders = { ...reminders, status: 'loading', error: null };
    reminders = await loadReminders(session, reminders.actionIds);
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
        await session.register(username, password, email, inviteCode);
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
    await refreshBatchDetail(batch.id);
  }

  async function openReminder(reminder: Reminder) {
    if (!session) {
      return;
    }
    const id = reminder.id;
    reminders = {
      ...reminders,
      actionIds: new Set([...reminders.actionIds, id]),
      error: null
    };
    try {
      await session.remindersOpen(id);
      const batchId = reminderBatchId(reminder);
      highlightBatchId = batchId;
      await refreshInventory(batchId);
      await refreshReminders();
    } catch {
      reminders = { ...reminders, error: 'Reminder could not be opened.' };
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
    } catch {
      reminders = optimisticAckRollback(reminders, reminder, 'Reminder could not be acknowledged.');
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

  async function discardSelected() {
    if (!session || !selectedBatch) {
      return;
    }
    stockActionBusy = 'discard';
    stockActionError = null;
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

  function clearHouseholdState() {
    inventory = emptyInventoryState;
    reminders = emptyReminderState;
    locationNames = {};
    selectedBatch = null;
    selectedBatchId = null;
    history = emptyHistoryState;
    consumeQuantity = '';
    stockActionBusy = null;
    stockActionError = null;
    highlightBatchId = null;
  }

  function formatDateTime(value: string | undefined | null): string {
    if (!value) {
      return '';
    }
    const parsed = Date.parse(value);
    return Number.isNaN(parsed) ? value : new Date(parsed).toLocaleString();
  }

  function displayLocation(batch: StockBatch): string {
    const id = stockLocationId(batch);
    return (id ? locationNames[id] : null) ?? stockLocation(batch);
  }
</script>

<svelte:head>
  <title>Quartermaster</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">Quartermaster</p>
      <h1>Kitchen inventory</h1>
    </div>
    {#if authenticated}
      <button class="ghost-button" type="button" onclick={logout}>Log out</button>
    {/if}
  </header>

  {#if !authenticated}
    <section class="auth-layout">
      <form class="panel auth-panel" onsubmit={(event) => { event.preventDefault(); void submitAuth(); }}>
        <div class="segmented">
          <button class:active={authMode === 'login'} type="button" onclick={() => (authMode = 'login')}>Login</button>
          <button class:active={authMode === 'register'} type="button" onclick={() => (authMode = 'register')}>Register</button>
        </div>

        <label>
          Server URL
          <input bind:value={serverUrl} placeholder="http://localhost:8080" autocomplete="url" />
        </label>
        <label>
          Username
          <input bind:value={username} autocomplete="username" required />
        </label>
        <label>
          Password
          <input bind:value={password} type="password" autocomplete={authMode === 'login' ? 'current-password' : 'new-password'} required minlength="8" />
        </label>

        {#if authMode === 'register'}
          <label>
            Email
            <input bind:value={email} type="email" autocomplete="email" />
          </label>
          <label>
            Invite code
            <input bind:value={inviteCode} autocomplete="one-time-code" />
          </label>
        {/if}

        {#if authError}
          <p class="error-text">{authError}</p>
        {/if}

        <button class="primary-action" type="submit" disabled={authBusy || !username || password.length < 8}>
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
            <button type="button" onclick={() => switchHousehold(household.id)}>{household.name}</button>
          {/each}
        </div>
      {:else}
        <p class="muted">Create or join a household from a native app for now.</p>
      {/if}
    </section>
  {:else if me && activeHousehold}
    <section class="workspace">
      <aside class="sidebar">
        <div>
          <p class="eyebrow">Current household</p>
          <h2>{activeHousehold.name}</h2>
        </div>
        {#if households.length > 1}
          <label>
            Switch household
            <select onchange={(event) => switchHousehold(event.currentTarget.value)} value={activeHousehold.id}>
              {#each households as household}
                <option value={household.id}>{household.name}</option>
              {/each}
            </select>
          </label>
        {/if}
        <button class="secondary-action" type="button" onclick={() => refreshWorkspace(selectedBatchId)}>Refresh</button>

        <section class="inbox-region" aria-labelledby="reminder-heading">
          <div class="section-heading compact">
            <div>
              <p class="eyebrow">Due now</p>
              <h2 id="reminder-heading">Reminders</h2>
            </div>
            <span>{reminders.items.length}</span>
          </div>

          {#if reminders.status === 'loading'}
            <p class="muted">Loading reminders...</p>
          {:else if reminders.status === 'error'}
            <p class="error-text">{reminders.error}</p>
          {:else if reminders.items.length === 0}
            <p class="muted">No due reminders.</p>
          {:else}
            <div class="reminder-list">
              {#each reminders.items as reminder}
                <article class="reminder-row">
                  <div>
                    <h3>{reminder.title}</h3>
                    <p>{reminder.body}</p>
                    <span>{reminderFireAt(reminder)}</span>
                  </div>
                  <div class="row-actions">
                    <button class="secondary-action small" type="button" disabled={reminders.actionIds.has(reminder.id)} onclick={() => openReminder(reminder)}>Open</button>
                    <button class="ghost-button small" type="button" disabled={reminders.actionIds.has(reminder.id)} onclick={() => ackReminder(reminder)}>Ack</button>
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
          <span>{inventory.items.length} batches</span>
        </div>

        {#if inventory.status === 'loading'}
          <p class="muted">Loading inventory...</p>
        {:else if inventory.status === 'error'}
          <p class="error-text">{inventory.error}</p>
        {:else if inventory.items.length === 0}
          <p class="muted">No stock is currently visible for this household.</p>
        {:else}
          <div class="inventory-list">
            {#each inventory.items as batch}
              <button
                class:active={selectedBatchId === batch.id}
                class:depleted={isDepleted(batch)}
                class:highlight={highlightBatchId === batch.id}
                class="stock-row"
                type="button"
                onclick={() => selectBatch(batch)}
              >
                <div>
                  <h3>{stockName(batch)}</h3>
                  <p>{displayLocation(batch)} · Expires {stockExpiry(batch)}</p>
                </div>
                <strong>{batch.quantity ?? '?'} {stockUnit(batch)}</strong>
              </button>
            {/each}
          </div>
        {/if}
      </section>

      <section class="detail-region">
        {#if selectedBatch}
          <div class="section-heading">
            <div>
              <p class="eyebrow">{isDepleted(selectedBatch) ? 'Depleted' : 'In stock'}</p>
              <h2>{stockName(selectedBatch)}</h2>
            </div>
            <strong data-testid="detail-quantity">{selectedBatch.quantity ?? '?'} {stockUnit(selectedBatch)}</strong>
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
            <div>
              <dt>Note</dt>
              <dd>{selectedBatch.note || 'None'}</dd>
            </div>
          </dl>

          <form class="action-panel" onsubmit={(event) => { event.preventDefault(); void consumeSelected(); }}>
            <label>
              Consume quantity
              <input bind:value={consumeQuantity} inputmode="decimal" placeholder={`Amount in ${stockUnit(selectedBatch)}`} disabled={isDepleted(selectedBatch)} />
            </label>
            <div class="stock-actions">
              <button class="primary-action" type="submit" disabled={stockActionBusy !== null || isDepleted(selectedBatch)}>Consume</button>
              <button class="secondary-action" type="button" disabled={stockActionBusy !== null || isDepleted(selectedBatch)} onclick={discardSelected}>Discard</button>
              {#if restoreAvailable}
                <button class="secondary-action" type="button" disabled={stockActionBusy !== null} onclick={restoreSelected}>Restore</button>
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
                      <p>{eventActor(event)} · {formatDateTime(eventCreated(event))}</p>
                      {#if event.note}
                        <p>{event.note}</p>
                      {/if}
                    </div>
                    <strong>{eventDelta(event)} {event.unit}</strong>
                  </article>
                {/each}
              </div>
              {#if history.nextBefore && history.nextBeforeId}
                <button class="secondary-action" type="button" disabled={history.status === 'loading'} onclick={loadMoreHistory}>Load more</button>
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
