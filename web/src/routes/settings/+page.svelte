<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { generatedTransport } from '$lib/api';
  import {
    buildCreateLocationRequest,
    buildUpdateLocationRequest,
    locationDeleteErrorMessage,
    locationKinds,
    locationSortOrder,
    normalizeLocationKind,
    sortLocations,
    validateLocationName,
    type LocationKind
  } from '$lib/locations';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type Location,
    type MeResponse
  } from '$lib/session-core';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let locations = $state<Location[]>([]);
  let actionBusy = $state<string | null>(null);
  let formError = $state<string | null>(null);
  let deleteError = $state<string | null>(null);
  let editingLocation = $state<Location | null>(null);
  let pendingDelete = $state<Location | null>(null);
  let locationName = $state('');
  let locationKind = $state<LocationKind>('pantry');

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const sortedLocations = $derived(sortLocations(locations));

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
    void loadSettings();
  });

  async function loadSettings() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
      if (currentHousehold(me)) {
        await refreshLocations();
      } else {
        locations = [];
      }
    } catch {
      me = null;
      locations = [];
      authenticated = false;
      error = 'Sign in again to continue.';
    } finally {
      loading = false;
    }
  }

  async function refreshLocations() {
    if (!session) {
      return;
    }
    locations = sortLocations(await session.locationsList());
  }

  function startCreate() {
    editingLocation = null;
    pendingDelete = null;
    locationName = '';
    locationKind = 'pantry';
    formError = null;
    deleteError = null;
  }

  function startEdit(location: Location) {
    editingLocation = location;
    pendingDelete = null;
    locationName = location.name;
    locationKind = normalizeLocationKind(location.kind ?? 'pantry');
    formError = null;
    deleteError = null;
  }

  function cancelEdit() {
    startCreate();
  }

  async function saveLocation() {
    if (!session) {
      return;
    }
    const validation = validateLocationName(locationName);
    if (validation) {
      formError = validation;
      return;
    }
    const busyId = editingLocation ? `save:${editingLocation.id}` : 'create';
    actionBusy = busyId;
    formError = null;
    try {
      if (editingLocation) {
        await session.locationsUpdate(
          editingLocation.id,
          buildUpdateLocationRequest(editingLocation, {
            name: locationName,
            kind: locationKind
          })
        );
      } else {
        await session.locationsCreate(
          buildCreateLocationRequest({ name: locationName, kind: locationKind })
        );
      }
      await refreshLocations();
      startCreate();
    } catch {
      formError = editingLocation ? 'Location could not be saved.' : 'Location could not be added.';
    } finally {
      actionBusy = null;
    }
  }

  async function moveLocation(location: Location, direction: -1 | 1) {
    if (!session) {
      return;
    }
    const current = sortedLocations;
    const index = current.findIndex((item) => item.id === location.id);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= current.length) {
      return;
    }
    const reordered = [...current];
    reordered.splice(target, 0, reordered.splice(index, 1)[0]);
    actionBusy = `move:${location.id}`;
    error = null;
    try {
      await Promise.all(
        reordered.map((item, sortOrder) =>
          session!.locationsUpdate(item.id, {
            name: item.name,
            kind: normalizeLocationKind(item.kind ?? 'pantry'),
            sort_order: sortOrder
          })
        )
      );
      await refreshLocations();
    } catch {
      error = 'Locations could not be reordered.';
    } finally {
      actionBusy = null;
    }
  }

  function confirmDelete(location: Location) {
    pendingDelete = location;
    deleteError = null;
    formError = null;
  }

  async function deleteLocation() {
    if (!session || !pendingDelete) {
      return;
    }
    const deleting = pendingDelete;
    actionBusy = `delete:${deleting.id}`;
    deleteError = null;
    try {
      await session.locationsDelete(deleting.id);
      pendingDelete = null;
      if (editingLocation?.id === deleting.id) {
        startCreate();
      }
      await refreshLocations();
    } catch (err) {
      deleteError = locationDeleteErrorMessage(err);
    } finally {
      actionBusy = null;
    }
  }

  async function logout() {
    if (!session) {
      return;
    }
    await session.logout();
    authenticated = false;
    me = null;
    locations = [];
    await goto('./');
  }
</script>

<svelte:head>
  <title>Settings · Quartermaster</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">Quartermaster</p>
      <h1>Settings</h1>
    </div>
    <div class="heading-actions">
      <a class="secondary-action" href="/">Inventory</a>
      <a class="secondary-action" href="/products">Products</a>
      {#if authenticated}
        <button class="ghost-button" type="button" onclick={logout}>Log out</button>
      {/if}
    </div>
  </header>

  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading settings...</p>
    </section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <p class="muted">Open the inventory screen and sign in before editing household settings.</p>
      <a class="primary-action" href="./">Go to inventory</a>
      {#if error}
        <p class="error-text">{error}</p>
      {/if}
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p class="muted">Switch to a household from the inventory screen before editing locations.</p>
      <a class="primary-action" href="./">Go to inventory</a>
    </section>
  {:else}
    <section class="settings-layout">
      <section class="panel settings-panel" aria-labelledby="locations-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Household</p>
            <h2 id="locations-heading">Locations</h2>
          </div>
          <button class="secondary-action small" type="button" onclick={() => refreshLocations()}
            >Refresh</button
          >
        </div>

        {#if error}
          <p class="error-text">{error}</p>
        {/if}

        {#if sortedLocations.length === 0}
          <p class="muted">No locations yet.</p>
        {:else}
          <div class="location-list" data-testid="settings-location-list">
            {#each sortedLocations as location, index}
              <article class="location-row" data-testid={`location-row-${location.name}`}>
                <div>
                  <h3>{location.name}</h3>
                  <p>{location.kind ?? 'pantry'} · order {locationSortOrder(location) + 1}</p>
                </div>
                <div class="row-actions">
                  <button
                    class="ghost-button small"
                    type="button"
                    data-testid={`location-move-up-${location.name}`}
                    disabled={index === 0 || actionBusy !== null}
                    onclick={() => moveLocation(location, -1)}>Up</button
                  >
                  <button
                    class="ghost-button small"
                    type="button"
                    data-testid={`location-move-down-${location.name}`}
                    disabled={index === sortedLocations.length - 1 || actionBusy !== null}
                    onclick={() => moveLocation(location, 1)}>Down</button
                  >
                  <button
                    class="secondary-action small"
                    type="button"
                    data-testid={`location-edit-${location.name}`}
                    disabled={actionBusy !== null}
                    onclick={() => startEdit(location)}>Edit</button
                  >
                  <button
                    class="ghost-button small danger"
                    type="button"
                    data-testid={`location-delete-${location.name}`}
                    disabled={actionBusy !== null}
                    onclick={() => confirmDelete(location)}>Delete</button
                  >
                </div>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      <aside class="panel settings-panel">
        <form
          class="location-form"
          data-testid="location-form"
          onsubmit={(event) => {
            event.preventDefault();
            void saveLocation();
          }}
        >
          <div class="section-heading compact">
            <div>
              <p class="eyebrow">{editingLocation ? 'Edit' : 'New'}</p>
              <h2>{editingLocation ? editingLocation.name : 'Add location'}</h2>
            </div>
            {#if editingLocation}
              <button class="ghost-button small" type="button" onclick={cancelEdit}>Cancel</button>
            {/if}
          </div>
          <label>
            Name
            <input bind:value={locationName} data-testid="location-name-input" maxlength="64" />
          </label>
          <label>
            Kind
            <select bind:value={locationKind} data-testid="location-kind-select">
              {#each locationKinds as kind}
                <option value={kind}>{kind}</option>
              {/each}
            </select>
          </label>
          {#if formError}
            <p class="error-text">{formError}</p>
          {/if}
          <button
            class="primary-action"
            type="submit"
            data-testid={editingLocation ? 'location-save-edit' : 'location-create'}
            disabled={actionBusy !== null}
          >
            {actionBusy === 'create'
              ? 'Adding...'
              : editingLocation
                ? 'Save location'
                : 'Add location'}
          </button>
        </form>

        {#if pendingDelete}
          <div class="delete-confirmation" data-testid="location-delete-confirmation">
            <h2>Delete {pendingDelete.name}?</h2>
            <p class="muted">This location will be removed if it has no active stock.</p>
            <div class="row-actions">
              <button
                class="ghost-button danger"
                type="button"
                data-testid="location-delete-confirm"
                disabled={actionBusy !== null}
                onclick={deleteLocation}>Delete location</button
              >
              <button
                class="secondary-action"
                type="button"
                disabled={actionBusy !== null}
                onclick={() => (pendingDelete = null)}>Cancel</button
              >
            </div>
            {#if deleteError}
              <p class="error-text">{deleteError}</p>
            {/if}
          </div>
        {/if}
      </aside>
    </section>
  {/if}
</main>
