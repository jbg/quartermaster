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
    type LabelPrinterDelivery,
    type LabelPrinterMedia,
    type Location,
    type MeResponse,
    type OpenFoodFactsCredentialStatusResponse,
    type StorageVessel
  } from '$lib/session-core';
  import type {
    HouseholdDetailDto,
    HouseholdExportDocument,
    MeasurementSystem
  } from '$lib/generated/types.gen';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let locations = $state<Location[]>([]);
  let storageVessels = $state<StorageVessel[]>([]);
  let printers = $state<LabelPrinter[]>([]);
  let actionBusy = $state<string | null>(null);
  let formError = $state<string | null>(null);
  let printerError = $state<string | null>(null);
  let printerMessage = $state<string | null>(null);
  let vesselError = $state<string | null>(null);
  let vesselDeleteError = $state<string | null>(null);
  let offCredentialStatus = $state<OpenFoodFactsCredentialStatusResponse | null>(null);
  let offUsername = $state('');
  let offPassword = $state('');
  let offMessage = $state<string | null>(null);
  let offError = $state<string | null>(null);
  let deleteError = $state<string | null>(null);
  let householdDataMessage = $state<string | null>(null);
  let householdDataError = $state<string | null>(null);
  let householdDetail = $state<HouseholdDetailDto | null>(null);
  let householdName = $state('');
  let householdTimezone = $state('');
  let householdMeasurementSystem = $state<MeasurementSystem>('metric');
  let householdDetailsMessage = $state<string | null>(null);
  let householdDetailsError = $state<string | null>(null);
  let deletionConfirmationName = $state('');
  let importInput: HTMLInputElement | null = $state(null);
  let editingLocation = $state<Location | null>(null);
  let editingVessel = $state<StorageVessel | null>(null);
  let editingPrinter = $state<LabelPrinter | null>(null);
  let pendingDelete = $state<Location | null>(null);
  let pendingVesselDelete = $state<StorageVessel | null>(null);
  let locationName = $state('');
  let locationKind = $state<LocationKind>('pantry');
  let vesselName = $state('');
  let vesselTareWeight = $state('');
  let vesselTareUnit = $state('g');
  let pairingServerUrl = $state('');
  let pairingQrSvg = $state('');
  let printerName = $state('');
  let printerAddress = $state('');
  let printerPort = $state('9100');
  let printerMedia = $state<LabelPrinterMedia>('dk_62_continuous');
  let printerDelivery = $state<LabelPrinterDelivery>('server');
  let printerDefault = $state(false);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const currentMembership = $derived(
    activeHousehold ? households.find((household) => household.id === activeHousehold.id) : null
  );
  const isAdmin = $derived(currentMembership?.role === 'admin');
  const sortedLocations = $derived(sortLocations(locations));
  const sortedStorageVessels = $derived(sortStorageVessels(storageVessels));
  const inventoryHref = $derived(appPath('/', page.url));
  const mobilePairingServerUrl = $derived(
    me?.public_base_url?.trim() || me?.publicBaseUrl?.trim() || pairingServerUrl
  );
  const pairingDeepLink = $derived(quartermasterServerUrl(mobilePairingServerUrl));
  const measurementSystemOptions: Array<{
    value: MeasurementSystem;
    label: string;
    detail: string;
  }> = [
    { value: 'metric', label: 'Metric', detail: '1 tsp = 5 ml, 1 tbsp = 15 ml' },
    {
      value: 'us_customary',
      label: 'US customary',
      detail: '1 tsp = 4.929 ml, 1 tbsp = 14.787 ml'
    },
    { value: 'australian', label: 'Australian', detail: '1 tsp = 5 ml, 1 tbsp = 20 ml' },
    { value: 'imperial', label: 'Imperial', detail: '1 tsp = 5.919 ml, 1 tbsp = 17.758 ml' }
  ];
  const selectedMeasurementSystem = $derived(
    measurementSystemOptions.find((option) => option.value === householdMeasurementSystem) ??
      measurementSystemOptions[0]
  );

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
        await Promise.all([
          refreshHouseholdDetail(),
          refreshLocations(),
          refreshStorageVessels(),
          refreshPrinters(),
          refreshOffCredentialStatus()
        ]);
      } else {
        locations = [];
        storageVessels = [];
        printers = [];
        offCredentialStatus = null;
        householdDetail = null;
      }
    } catch {
      me = null;
      locations = [];
      storageVessels = [];
      printers = [];
      householdDetail = null;
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

  async function refreshHouseholdDetail() {
    if (!session) {
      return;
    }
    householdDetail = await session.householdCurrentGet();
    householdName = householdDetail.name;
    householdTimezone = householdDetail.timezone;
    householdMeasurementSystem = householdDetail.measurement_system;
  }

  async function saveHouseholdDetails() {
    if (!session || !householdDetail) {
      return;
    }
    const name = householdName.trim();
    const timezone = householdTimezone.trim();
    if (!name || !timezone) {
      householdDetailsError = 'Enter a household name and timezone.';
      return;
    }
    actionBusy = 'household:details';
    householdDetailsError = null;
    householdDetailsMessage = null;
    try {
      householdDetail = await session.householdCurrentUpdate({
        name,
        timezone,
        measurement_system: householdMeasurementSystem
      });
      householdName = householdDetail.name;
      householdTimezone = householdDetail.timezone;
      householdMeasurementSystem = householdDetail.measurement_system;
      me = await session.me();
      householdDetailsMessage = 'Household details saved.';
    } catch (err) {
      householdDetailsError =
        err instanceof Error ? err.message : 'Household details could not be saved.';
    } finally {
      actionBusy = null;
    }
  }

  function resetHouseholdDetails() {
    if (!householdDetail) {
      return;
    }
    householdName = householdDetail.name;
    householdTimezone = householdDetail.timezone;
    householdMeasurementSystem = householdDetail.measurement_system;
    householdDetailsError = null;
    householdDetailsMessage = null;
  }

  async function refreshStorageVessels() {
    if (!session) {
      return;
    }
    storageVessels = sortStorageVessels(await session.storageVesselsList());
  }

  async function switchHousehold(id: string) {
    if (!session) {
      return;
    }
    try {
      me = await session.switchHousehold(id);
      await loadSettings();
    } catch {
      error = 'Household could not be switched.';
    }
  }

  function beginImportBackup() {
    householdDataError = null;
    householdDataMessage = null;
    importInput?.click();
  }

  async function importBackupFile(file: File | null | undefined) {
    if (!session || !file) {
      return;
    }
    actionBusy = 'household:import';
    householdDataError = null;
    householdDataMessage = null;
    try {
      const document = JSON.parse(await file.text()) as HouseholdExportDocument;
      me = await session.householdImport(document);
      householdDataMessage = 'Backup imported.';
      await loadSettings();
    } catch (err) {
      householdDataError =
        err instanceof SyntaxError
          ? 'Choose a valid Quartermaster JSON backup.'
          : err instanceof Error
            ? err.message
            : 'Backup could not be imported.';
    } finally {
      actionBusy = null;
      if (importInput) {
        importInput.value = '';
      }
    }
  }

  async function exportBackup() {
    if (!session || !activeHousehold) {
      return;
    }
    actionBusy = 'household:export';
    householdDataError = null;
    householdDataMessage = null;
    try {
      const document = await session.householdCurrentExport();
      const json = JSON.stringify(document, null, 2);
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const link = documentLink(url, backupFileName(activeHousehold.name));
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
      householdDataMessage = 'Backup exported.';
    } catch (err) {
      householdDataError = err instanceof Error ? err.message : 'Backup could not be exported.';
    } finally {
      actionBusy = null;
    }
  }

  async function deleteHousehold() {
    if (!session || !activeHousehold || deletionConfirmationName !== activeHousehold.name) {
      return;
    }
    actionBusy = 'household:delete';
    householdDataError = null;
    householdDataMessage = null;
    try {
      await session.householdCurrentDeletionRequest(deletionConfirmationName);
      me = await session.me();
      locations = [];
      storageVessels = [];
      printers = [];
      offCredentialStatus = null;
      deletionConfirmationName = '';
      householdDataMessage = 'Household deletion queued.';
    } catch (err) {
      householdDataError = err instanceof Error ? err.message : 'Household could not be deleted.';
    } finally {
      actionBusy = null;
    }
  }

  async function refreshPrinters() {
    if (!session) {
      return;
    }
    const response = await session.labelPrintersList();
    printers = response.items ?? [];
  }

  async function refreshOffCredentialStatus() {
    if (!session) {
      return;
    }
    offCredentialStatus = await session.openFoodFactsCredentialStatus();
    offUsername = offCredentialStatus.username ?? '';
    offPassword = '';
  }

  async function saveOffCredentials() {
    if (!session) {
      return;
    }
    actionBusy = 'off';
    offError = null;
    offMessage = null;
    try {
      offCredentialStatus = await session.saveOpenFoodFactsCredentials({
        username: offUsername,
        password: offPassword
      });
      offUsername = offCredentialStatus.username ?? '';
      offPassword = '';
      offMessage = 'OpenFoodFacts credentials saved.';
    } catch (err) {
      offError =
        err instanceof Error ? err.message : 'OpenFoodFacts credentials could not be saved.';
    } finally {
      actionBusy = null;
    }
  }

  async function deleteOffCredentials() {
    if (!session) {
      return;
    }
    actionBusy = 'off';
    offError = null;
    offMessage = null;
    try {
      await session.deleteOpenFoodFactsCredentials();
      offCredentialStatus = { configured: false, username: null };
      offUsername = '';
      offPassword = '';
      offMessage = 'OpenFoodFacts credentials removed.';
    } catch (err) {
      offError =
        err instanceof Error ? err.message : 'OpenFoodFacts credentials could not be removed.';
    } finally {
      actionBusy = null;
    }
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

  function resetPrinterForm() {
    editingPrinter = null;
    printerName = '';
    printerAddress = '';
    printerPort = '9100';
    printerMedia = 'dk_62_continuous';
    printerDelivery = 'server';
    printerDefault = false;
    printerError = null;
    printerMessage = null;
  }

  function resetVesselForm() {
    editingVessel = null;
    pendingVesselDelete = null;
    vesselName = '';
    vesselTareWeight = '';
    vesselTareUnit = 'g';
    vesselError = null;
    vesselDeleteError = null;
  }

  function startEditVessel(vessel: StorageVessel) {
    editingVessel = vessel;
    pendingVesselDelete = null;
    vesselName = vessel.name;
    vesselTareWeight = String(vessel.tare_weight ?? vessel.tareWeight ?? '');
    vesselTareUnit = vessel.tare_unit ?? vessel.tareUnit ?? 'g';
    vesselError = null;
    vesselDeleteError = null;
  }

  function startEditPrinter(printer: LabelPrinter) {
    editingPrinter = printer;
    printerName = printer.name;
    printerAddress = printer.address;
    printerPort = String(printer.port);
    printerMedia = printer.media;
    printerDelivery = printer.delivery ?? 'server';
    printerDefault = printer.is_default || printer.isDefault || false;
    printerError = null;
    printerMessage = null;
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

  async function moveStorageVessel(vessel: StorageVessel, direction: -1 | 1) {
    if (!session) {
      return;
    }
    const current = sortedStorageVessels;
    const index = current.findIndex((item) => item.id === vessel.id);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= current.length) {
      return;
    }
    const reordered = [...current];
    reordered.splice(target, 0, reordered.splice(index, 1)[0]);
    actionBusy = `vessel:move:${vessel.id}`;
    vesselError = null;
    try {
      await Promise.all(
        reordered.map((item, sortOrder) =>
          session!.storageVesselsUpdate(item.id, {
            name: item.name,
            tare_weight: String(item.tare_weight ?? item.tareWeight ?? '0'),
            tare_unit: item.tare_unit ?? item.tareUnit ?? 'g',
            sort_order: sortOrder
          })
        )
      );
      await refreshStorageVessels();
    } catch {
      vesselError = 'Tare profiles could not be reordered.';
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

  async function saveStorageVessel() {
    if (!session) {
      return;
    }
    const name = vesselName.trim();
    const weight = vesselTareWeight.trim();
    const parsed = Number(weight);
    if (!name || !weight || !Number.isFinite(parsed) || parsed < 0) {
      vesselError = 'Enter a profile name and zero-or-greater tare weight.';
      return;
    }
    const busyId = editingVessel ? `vessel:save:${editingVessel.id}` : 'vessel:create';
    actionBusy = busyId;
    vesselError = null;
    try {
      if (editingVessel) {
        await session.storageVesselsUpdate(editingVessel.id, {
          name,
          tare_weight: weight,
          tare_unit: vesselTareUnit,
          sort_order: storageVesselSortOrder(editingVessel)
        });
      } else {
        await session.storageVesselsCreate({
          name,
          tare_weight: weight,
          tare_unit: vesselTareUnit
        });
      }
      await refreshStorageVessels();
      resetVesselForm();
    } catch {
      vesselError = editingVessel
        ? 'Tare profile could not be saved.'
        : 'Tare profile could not be added.';
    } finally {
      actionBusy = null;
    }
  }

  function confirmVesselDelete(vessel: StorageVessel) {
    pendingVesselDelete = vessel;
    vesselDeleteError = null;
    vesselError = null;
  }

  async function deleteStorageVessel() {
    if (!session || !pendingVesselDelete) {
      return;
    }
    const deleting = pendingVesselDelete;
    actionBusy = `vessel:delete:${deleting.id}`;
    vesselDeleteError = null;
    try {
      await session.storageVesselsDelete(deleting.id);
      pendingVesselDelete = null;
      if (editingVessel?.id === deleting.id) {
        resetVesselForm();
      }
      await refreshStorageVessels();
    } catch {
      vesselDeleteError = 'Tare profile could not be deleted.';
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
    const busyId = editingPrinter ? `printer:save:${editingPrinter.id}` : 'printer:create';
    actionBusy = busyId;
    printerError = null;
    printerMessage = null;
    try {
      if (editingPrinter) {
        await session.labelPrintersUpdate(editingPrinter.id, {
          name,
          address,
          port,
          media: printerMedia,
          delivery: printerDelivery,
          is_default: printerDefault
        });
      } else {
        await session.labelPrintersCreate({
          name,
          driver: 'brother_ql_raster',
          address,
          port,
          media: printerMedia,
          delivery: printerDelivery,
          enabled: true,
          is_default: printerDefault || printers.length === 0
        });
      }
      resetPrinterForm();
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
      if (printer.delivery === 'client') {
        await session.labelPrintersTestRender(printer.id);
        printerMessage =
          'Test label rendered. Open iOS or Android on the printer network to send it.';
      } else {
        await session.labelPrintersTest(printer.id);
        printerMessage = 'Test label sent.';
      }
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
      if (editingPrinter?.id === printer.id) {
        resetPrinterForm();
      }
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
    storageVessels = [];
    await goto(inventoryHref);
  }

  function storageVesselSortOrder(vessel: StorageVessel): number {
    return vessel.sort_order ?? vessel.sortOrder ?? 0;
  }

  function sortStorageVessels(items: StorageVessel[]): StorageVessel[] {
    return [...items].sort((a, b) => {
      const order = storageVesselSortOrder(a) - storageVesselSortOrder(b);
      return order !== 0 ? order : a.name.localeCompare(b.name);
    });
  }

  function backupFileName(householdName: string): string {
    const safeName = householdName
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '');
    return `quartermaster-${safeName || 'household'}-${new Date().toISOString().slice(0, 10)}.json`;
  }

  function documentLink(url: string, filename: string): HTMLAnchorElement {
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.rel = 'noopener';
    document.body.append(link);
    return link;
  }
</script>

<svelte:head>
  <title>Settings · Quartermaster</title>
</svelte:head>

<AppFrame
  title="Settings"
  {authenticated}
  active="settings"
  {activeHousehold}
  {households}
  onhouseholdchange={switchHousehold}
  onimportbackup={beginImportBackup}
  onlogout={logout}
>
  <input
    bind:this={importInput}
    class="visually-hidden"
    type="file"
    accept="application/json,.json"
    onchange={(event) => {
      void importBackupFile(event.currentTarget.files?.[0]);
    }}
  />
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
      <p class="muted">Switch to a household from the inventory screen or import a backup.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      <button
        class="secondary-action"
        type="button"
        disabled={actionBusy !== null}
        onclick={beginImportBackup}>Import backup</button
      >
      {#if householdDataError}
        <p class="error-text">{householdDataError}</p>
      {/if}
    </section>
  {:else}
    <section class="settings-layout">
      <section class="panel settings-panel" aria-labelledby="household-details-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Household</p>
            <h2 id="household-details-heading">Details</h2>
          </div>
        </div>

        {#if householdDetailsMessage}
          <p class="muted">{householdDetailsMessage}</p>
        {/if}
        {#if householdDetailsError}
          <p class="error-text">{householdDetailsError}</p>
        {/if}

        {#if isAdmin && householdDetail}
          <form
            class="settings-form"
            onsubmit={(event) => {
              event.preventDefault();
              void saveHouseholdDetails();
            }}
          >
            <label>
              Household name
              <input bind:value={householdName} maxlength="120" />
            </label>
            <label>
              Timezone
              <input bind:value={householdTimezone} placeholder="Europe/Madrid" />
            </label>
            <label>
              Measurement system
              <select bind:value={householdMeasurementSystem}>
                {#each measurementSystemOptions as option}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>
            <p class="muted">{selectedMeasurementSystem.detail}</p>
            <div class="row-actions">
              <button
                class="primary-action"
                type="submit"
                disabled={actionBusy !== null || !householdName.trim() || !householdTimezone.trim()}
                >{actionBusy === 'household:details' ? 'Saving...' : 'Save household'}</button
              >
              <button
                class="ghost-button"
                type="button"
                disabled={actionBusy !== null}
                onclick={resetHouseholdDetails}>Reset</button
              >
            </div>
          </form>
        {:else}
          <div class="detail-grid compact">
            <div>
              <h3>Name</h3>
              <p>{householdDetail?.name ?? activeHousehold?.name ?? 'Unknown'}</p>
            </div>
            <div>
              <h3>Timezone</h3>
              <p>{householdDetail?.timezone ?? 'UTC'}</p>
            </div>
            <div>
              <h3>Measurement system</h3>
              <p>{selectedMeasurementSystem.label}</p>
            </div>
          </div>
          <p class="muted">{selectedMeasurementSystem.detail}</p>
        {/if}
      </section>

      <section class="panel settings-panel" aria-labelledby="locations-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Household</p>
            <h2 id="locations-heading">Locations</h2>
          </div>
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

      <section class="panel settings-panel" aria-labelledby="vessels-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Stocktake</p>
            <h2 id="vessels-heading">Tare profiles</h2>
          </div>
        </div>

        {#if vesselError}
          <p class="error-text">{vesselError}</p>
        {/if}

        {#if sortedStorageVessels.length === 0}
          <p class="muted">No tare profiles yet.</p>
        {:else}
          <div class="location-list">
            {#each sortedStorageVessels as vessel, index}
              <article class="location-row">
                <div>
                  <h3>{vessel.name}</h3>
                  <p>
                    {vessel.tare_weight ?? vessel.tareWeight}
                    {vessel.tare_unit ?? vessel.tareUnit}
                    - order {storageVesselSortOrder(vessel) + 1}
                  </p>
                </div>
                <div class="row-actions">
                  <button
                    class="ghost-button small"
                    type="button"
                    disabled={index === 0 || actionBusy !== null}
                    onclick={() => moveStorageVessel(vessel, -1)}>Up</button
                  >
                  <button
                    class="ghost-button small"
                    type="button"
                    disabled={index === sortedStorageVessels.length - 1 || actionBusy !== null}
                    onclick={() => moveStorageVessel(vessel, 1)}>Down</button
                  >
                  <button
                    class="secondary-action small"
                    type="button"
                    disabled={actionBusy !== null}
                    onclick={() => startEditVessel(vessel)}>Edit</button
                  >
                  <button
                    class="ghost-button small danger"
                    type="button"
                    disabled={actionBusy !== null}
                    onclick={() => confirmVesselDelete(vessel)}>Delete</button
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
            void saveStorageVessel();
          }}
        >
          <div class="section-heading compact">
            <div>
              <p class="eyebrow">{editingVessel ? 'Edit' : 'New'}</p>
              <h2>{editingVessel ? editingVessel.name : 'Add tare profile'}</h2>
            </div>
            {#if editingVessel}
              <button class="ghost-button small" type="button" onclick={resetVesselForm}
                >Cancel</button
              >
            {/if}
          </div>
          <label>
            Name
            <input bind:value={vesselName} maxlength="80" placeholder="1L Mason jar" />
          </label>
          <label>
            Tare weight
            <input bind:value={vesselTareWeight} inputmode="decimal" placeholder="410" />
          </label>
          <label>
            Tare unit
            <select bind:value={vesselTareUnit}>
              <option value="g">g</option>
              <option value="kg">kg</option>
              <option value="oz">oz</option>
              <option value="lb">lb</option>
            </select>
          </label>
          <button class="primary-action" type="submit" disabled={actionBusy !== null}
            >{editingVessel ? 'Save profile' : 'Add profile'}</button
          >
        </form>

        {#if pendingVesselDelete}
          <div class="delete-confirmation">
            <h2>Delete {pendingVesselDelete.name}?</h2>
            <p class="muted">Batches using this tare profile will keep their stock quantity.</p>
            <div class="row-actions">
              <button
                class="ghost-button danger"
                type="button"
                disabled={actionBusy !== null}
                onclick={deleteStorageVessel}>Delete profile</button
              >
              <button
                class="secondary-action"
                type="button"
                disabled={actionBusy !== null}
                onclick={() => (pendingVesselDelete = null)}>Cancel</button
              >
            </div>
            {#if vesselDeleteError}
              <p class="error-text">{vesselDeleteError}</p>
            {/if}
          </div>
        {/if}
      </section>

      <section class="panel settings-panel" aria-labelledby="printers-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Labels</p>
            <h2 id="printers-heading">Label printers</h2>
          </div>
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
                    - {printer.delivery === 'client' ? 'client-reached' : 'server-reached'}
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
                    disabled={actionBusy !== null}
                    onclick={() => startEditPrinter(printer)}>Edit</button
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
          <div class="section-heading compact">
            <div>
              <p class="eyebrow">{editingPrinter ? 'Edit' : 'New'}</p>
              <h2>{editingPrinter ? editingPrinter.name : 'Add printer'}</h2>
            </div>
            {#if editingPrinter}
              <button class="ghost-button small" type="button" onclick={resetPrinterForm}
                >Cancel</button
              >
            {/if}
          </div>
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
              <option value="dk_62_red_black_continuous">DK 62 red/black continuous</option>
              <option value="dk_29x90">DK 29 x 90</option>
            </select>
          </label>
          <label>
            Delivery
            <select bind:value={printerDelivery}>
              <option value="server">Server reaches printer</option>
              <option value="client">Phone or tablet reaches printer</option>
            </select>
          </label>
          <label class="checkbox-row">
            <input bind:checked={printerDefault} type="checkbox" />
            Use as default printer
          </label>
          <button class="primary-action" type="submit" disabled={actionBusy !== null}
            >{editingPrinter ? 'Save printer' : 'Add printer'}</button
          >
        </form>
      </section>

      <aside class="panel settings-panel">
        <section class="pairing-panel" aria-labelledby="household-data-heading">
          <div class="section-heading compact">
            <div>
              <p class="eyebrow">Backup</p>
              <h2 id="household-data-heading">Household data</h2>
            </div>
          </div>
          {#if householdDataMessage}
            <p class="muted">{householdDataMessage}</p>
          {/if}
          {#if householdDataError}
            <p class="error-text">{householdDataError}</p>
          {/if}
          <div class="row-actions">
            {#if isAdmin}
              <button
                class="primary-action"
                type="button"
                disabled={actionBusy !== null}
                onclick={exportBackup}>Export backup</button
              >
            {/if}
            <button
              class="secondary-action"
              type="button"
              disabled={actionBusy !== null}
              onclick={beginImportBackup}>Import backup</button
            >
          </div>
          {#if isAdmin && activeHousehold}
            <div class="delete-confirmation">
              <h2>Delete {activeHousehold.name}?</h2>
              <p class="muted">
                Export a backup first if you want to keep this data. Type the household name to
                queue deletion.
              </p>
              <label>
                Household name
                <input bind:value={deletionConfirmationName} autocomplete="off" />
              </label>
              <button
                class="ghost-button danger"
                type="button"
                disabled={actionBusy !== null || deletionConfirmationName !== activeHousehold.name}
                onclick={deleteHousehold}>Delete household</button
              >
            </div>
          {/if}
        </section>

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
          onsubmit={(event) => {
            event.preventDefault();
            void saveOffCredentials();
          }}
        >
          <div class="section-heading compact">
            <div>
              <p class="eyebrow">OpenFoodFacts</p>
              <h2>Contribution account</h2>
            </div>
          </div>
          <label>
            Username
            <input bind:value={offUsername} autocomplete="username" />
          </label>
          <label>
            Password
            <input bind:value={offPassword} autocomplete="current-password" type="password" />
          </label>
          {#if offCredentialStatus?.configured}
            <p class="muted">Saved for {offCredentialStatus.username}.</p>
          {/if}
          {#if offMessage}
            <p class="muted">{offMessage}</p>
          {/if}
          {#if offError}
            <p class="error-text">{offError}</p>
          {/if}
          <div class="row-actions">
            <button class="primary-action" type="submit" disabled={actionBusy !== null}>
              {actionBusy === 'off' ? 'Saving...' : 'Save credentials'}
            </button>
            {#if offCredentialStatus?.configured}
              <button
                class="ghost-button danger"
                type="button"
                disabled={actionBusy !== null}
                onclick={deleteOffCredentials}>Remove</button
              >
            {/if}
          </div>
        </form>

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
