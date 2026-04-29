<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import AppFrame from '$lib/components/AppFrame.svelte';
  import { generatedTransport } from '$lib/api';
  import { quartermasterServerUrl } from '$lib/join';
  import { appPath } from '$lib/paths';
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
    type LabelPrinter,
    type LabelPrinterMedia,
    type Location,
    type MeResponse
  } from '$lib/session-core';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let locations = $state<Location[]>([]);
  let printers = $state<LabelPrinter[]>([]);
  let actionBusy = $state<string | null>(null);
  let formError = $state<string | null>(null);
  let printerError = $state<string | null>(null);
  let printerMessage = $state<string | null>(null);
  let deleteError = $state<string | null>(null);
  let editingLocation = $state<Location | null>(null);
  let pendingDelete = $state<Location | null>(null);
  let locationName = $state('');
  let locationKind = $state<LocationKind>('pantry');
  let pairingServerUrl = $state('');
  let pairingQrSvg = $state('');
  let printerName = $state('');
  let printerAddress = $state('');
  let printerPort = $state('9100');
  let printerMedia = $state<LabelPrinterMedia>('dk_62_continuous');
  let printerDefault = $state(false);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const sortedLocations = $derived(sortLocations(locations));
  const inventoryHref = $derived(appPath('/', page.url));
  const mobilePairingServerUrl = $derived(
    me?.public_base_url?.trim() || me?.publicBaseUrl?.trim() || pairingServerUrl
  );
  const pairingDeepLink = $derived(quartermasterServerUrl(mobilePairingServerUrl));

  $effect(() => {
    if (!browser || !mobilePairingServerUrl) {
      pairingQrSvg = '';
      return;
    }
    const currentLink = pairingDeepLink;
    void import('qrcode')
      .then(({ toString: qrToString }) =>
        qrToString(currentLink, {
          type: 'svg',
          margin: 1,
          width: 208,
          color: {
            dark: '#173d32',
            light: '#ffffff'
          }
        })
      )
      .then((svg) => {
        if (pairingDeepLink === currentLink) {
          pairingQrSvg = svg;
        }
      });
  });

  onMount(() => {
    if (!browser) {
      return;
    }
    const created = new QuartermasterSession(
      createBrowserSessionStorage(window.localStorage, window.location),
      generatedTransport()
    );
    session = created;
    pairingServerUrl = created.snapshot().serverUrl;
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
        await Promise.all([refreshLocations(), refreshPrinters()]);
      } else {
        locations = [];
        printers = [];
      }
    } catch {
      me = null;
      locations = [];
      printers = [];
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

  async function refreshPrinters() {
    if (!session) {
      return;
    }
    const response = await session.labelPrintersList();
    printers = response.items ?? [];
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

  async function savePrinter() {
    if (!session) {
      return;
    }
    const name = printerName.trim();
    const address = printerAddress.trim();
    const port = Number(printerPort);
    if (!name || !address || !Number.isInteger(port) || port < 1 || port > 65535) {
      printerError = 'Enter a printer name, host, and valid port.';
      return;
    }
    actionBusy = 'printer:create';
    printerError = null;
    printerMessage = null;
    try {
      await session.labelPrintersCreate({
        name,
        driver: 'brother_ql_raster',
        address,
        port,
        media: printerMedia,
        enabled: true,
        is_default: printerDefault || printers.length === 0
      });
      printerName = '';
      printerAddress = '';
      printerPort = '9100';
      printerMedia = 'dk_62_continuous';
      printerDefault = false;
      await refreshPrinters();
      printerMessage = 'Printer saved.';
    } catch {
      printerError = 'Printer could not be saved.';
    } finally {
      actionBusy = null;
    }
  }

  async function setDefaultPrinter(printer: LabelPrinter) {
    if (!session) {
      return;
    }
    actionBusy = `printer:default:${printer.id}`;
    printerError = null;
    printerMessage = null;
    try {
      await session.labelPrintersUpdate(printer.id, { is_default: true, enabled: true });
      await refreshPrinters();
      printerMessage = 'Default printer updated.';
    } catch {
      printerError = 'Default printer could not be changed.';
    } finally {
      actionBusy = null;
    }
  }

  async function togglePrinter(printer: LabelPrinter) {
    if (!session) {
      return;
    }
    actionBusy = `printer:toggle:${printer.id}`;
    printerError = null;
    printerMessage = null;
    try {
      await session.labelPrintersUpdate(printer.id, { enabled: !printer.enabled });
      await refreshPrinters();
    } catch {
      printerError = 'Printer could not be updated.';
    } finally {
      actionBusy = null;
    }
  }

  async function testPrinter(printer: LabelPrinter) {
    if (!session) {
      return;
    }
    actionBusy = `printer:test:${printer.id}`;
    printerError = null;
    printerMessage = null;
    try {
      await session.labelPrintersTest(printer.id);
      printerMessage = 'Test label sent.';
    } catch {
      printerError = 'Test label could not be sent.';
    } finally {
      actionBusy = null;
    }
  }

  async function deletePrinter(printer: LabelPrinter) {
    if (!session) {
      return;
    }
    actionBusy = `printer:delete:${printer.id}`;
    printerError = null;
    printerMessage = null;
    try {
      await session.labelPrintersDelete(printer.id);
      await refreshPrinters();
      printerMessage = 'Printer deleted.';
    } catch {
      printerError = 'Printer could not be deleted.';
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
    await goto(inventoryHref);
  }
</script>

<svelte:head>
  <title>Settings · Quartermaster</title>
</svelte:head>

<AppFrame title="Settings" {authenticated} active="settings" onlogout={logout}>
  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading settings...</p>
    </section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <p class="muted">Open the inventory screen and sign in before editing household settings.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      {#if error}
        <p class="error-text">{error}</p>
      {/if}
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p class="muted">Switch to a household from the inventory screen before editing locations.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
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
                  <p>{location.kind ?? 'pantry'} - order {locationSortOrder(location) + 1}</p>
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

      <section class="panel settings-panel" aria-labelledby="printers-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Labels</p>
            <h2 id="printers-heading">Label printers</h2>
          </div>
          <button class="secondary-action small" type="button" onclick={() => refreshPrinters()}
            >Refresh</button
          >
        </div>

        {#if printerError}
          <p class="error-text">{printerError}</p>
        {/if}
        {#if printerMessage}
          <p class="muted">{printerMessage}</p>
        {/if}

        {#if printers.length === 0}
          <p class="muted">No label printers linked yet.</p>
        {:else}
          <div class="location-list">
            {#each printers as printer}
              <article class="location-row">
                <div>
                  <h3>{printer.name}</h3>
                  <p>
                    {printer.address}:{printer.port} - {printer.media}
                    {#if printer.is_default || printer.isDefault}
                      - default{/if}
                    {#if !printer.enabled}
                      - disabled{/if}
                  </p>
                </div>
                <div class="row-actions">
                  <button
                    class="secondary-action small"
                    type="button"
                    disabled={actionBusy !== null}
                    onclick={() => testPrinter(printer)}>Test</button
                  >
                  <button
                    class="ghost-button small"
                    type="button"
                    disabled={actionBusy !== null || printer.is_default || printer.isDefault}
                    onclick={() => setDefaultPrinter(printer)}>Default</button
                  >
                  <button
                    class="ghost-button small"
                    type="button"
                    disabled={actionBusy !== null}
                    onclick={() => togglePrinter(printer)}
                    >{printer.enabled ? 'Disable' : 'Enable'}</button
                  >
                  <button
                    class="ghost-button small danger"
                    type="button"
                    disabled={actionBusy !== null}
                    onclick={() => deletePrinter(printer)}>Delete</button
                  >
                </div>
              </article>
            {/each}
          </div>
        {/if}

        <form
          class="settings-form"
          onsubmit={(event) => {
            event.preventDefault();
            void savePrinter();
          }}
        >
          <label>
            Name
            <input bind:value={printerName} placeholder="Kitchen Brother" />
          </label>
          <label>
            Host or IP
            <input bind:value={printerAddress} placeholder="192.168.1.42" />
          </label>
          <label>
            Port
            <input bind:value={printerPort} inputmode="numeric" />
          </label>
          <label>
            Media
            <select bind:value={printerMedia}>
              <option value="dk_62_continuous">DK 62 continuous</option>
              <option value="dk_29x90">DK 29 x 90</option>
            </select>
          </label>
          <label class="checkbox-row">
            <input bind:checked={printerDefault} type="checkbox" />
            Use as default printer
          </label>
          <button class="primary-action" type="submit" disabled={actionBusy !== null}
            >Add printer</button
          >
        </form>
      </section>

      <aside class="panel settings-panel">
        <section class="pairing-panel" aria-labelledby="pairing-heading">
          <div class="section-heading compact">
            <div>
              <p class="eyebrow">Mobile</p>
              <h2 id="pairing-heading">Pair this server</h2>
            </div>
          </div>
          <p class="muted">
            Scan this from Quartermaster on a phone to set the server URL for sign-in.
          </p>
          {#if pairingQrSvg}
            <div class="pairing-qr" aria-label="Server pairing QR code">
              {@html pairingQrSvg}
            </div>
          {/if}
          <div class="detail-grid compact">
            <div>
              <h3>Server URL</h3>
              <code>{mobilePairingServerUrl}</code>
            </div>
            <div>
              <h3>App link</h3>
              <code>{pairingDeepLink}</code>
            </div>
          </div>
        </section>

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
</AppFrame>
