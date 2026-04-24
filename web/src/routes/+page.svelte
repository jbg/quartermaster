<script lang="ts">
  import { browser } from '$app/environment';
  import { currentHousehold, createBrowserSessionStorage, QuartermasterSession, type MeResponse } from '$lib/session-core';
  import { generatedTransport } from '$lib/api';
  import { loadInventory, stockExpiry, stockLocation, stockName, stockUnit, isDepleted, type InventoryState, emptyInventoryState } from '$lib/inventory';

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

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);

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
        await refreshInventory();
      } else {
        inventory = emptyInventoryState;
      }
    } catch {
      me = null;
      inventory = emptyInventoryState;
      authError = 'Sign in again to continue.';
    }
  }

  async function refreshInventory() {
    if (!session) {
      return;
    }
    inventory = { status: 'loading', items: inventory.items, error: null };
    inventory = await loadInventory(session);
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
    inventory = emptyInventoryState;
  }

  async function switchHousehold(id: string) {
    if (!session) {
      return;
    }
    authError = null;
    try {
      me = await session.switchHousehold(id);
      await refreshInventory();
    } catch {
      authError = 'Household could not be switched.';
    }
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
        <p class="eyebrow">Current household</p>
        <h2>{activeHousehold.name}</h2>
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
        <button class="secondary-action" type="button" onclick={refreshInventory}>Refresh inventory</button>
      </aside>

      <section class="inventory-region">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Read-only web shell</p>
            <h2>Inventory</h2>
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
              <article class:depleted={isDepleted(batch)} class="stock-row">
                <div>
                  <h3>{stockName(batch)}</h3>
                  <p>{stockLocation(batch)} · Expires {stockExpiry(batch)}</p>
                </div>
                <strong>{batch.quantity ?? '?'} {stockUnit(batch)}</strong>
              </article>
            {/each}
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
