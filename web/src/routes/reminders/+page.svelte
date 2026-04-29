<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import AppFrame from '$lib/components/AppFrame.svelte';
  import { generatedTransport } from '$lib/api';
  import { appPath } from '$lib/paths';
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
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type MeResponse,
    type Reminder
  } from '$lib/session-core';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let authenticated = $state(false);
  let loading = $state(true);
  let reminders = $state<ReminderState>(emptyReminderState);
  let error = $state<string | null>(null);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const inventoryHref = $derived(appPath('/', page.url));

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
    void loadPage();
  });

  async function loadPage() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
      if (currentHousehold(me)) {
        await refreshReminders();
      } else {
        reminders = emptyReminderState;
      }
    } catch {
      me = null;
      authenticated = false;
      reminders = emptyReminderState;
      error = 'Sign in again to continue.';
    } finally {
      loading = false;
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

  async function openReminder(reminder: Reminder) {
    if (!session) {
      return;
    }
    reminders = startReminderAction(reminders, reminder.id, 'open');
    try {
      await session.remindersOpen(reminder.id);
      await goto(appPath(`/?batch=${encodeURIComponent(reminderBatchId(reminder))}`, page.url));
    } catch {
      reminders = { ...reminders, error: reminderMessages.openError };
    } finally {
      reminders = actionDone(reminders, reminder.id);
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

  async function logout() {
    if (!session) {
      return;
    }
    await session.logout();
    authenticated = false;
    me = null;
    reminders = emptyReminderState;
    await goto(inventoryHref);
  }
</script>

<svelte:head>
  <title>Reminders · Quartermaster</title>
</svelte:head>

<AppFrame title="Reminders" {authenticated} active="reminders" onlogout={logout}>
  {#if loading}
    <section class="panel empty-state">
      <p class="muted">Loading reminders...</p>
    </section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <p class="muted">Open the inventory screen and sign in before reviewing reminders.</p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      {#if error}
        <p class="error-text">{error}</p>
      {/if}
    </section>
  {:else if me && !activeHousehold}
    <section class="panel empty-state">
      <h2>No household selected</h2>
      <p class="muted">
        Switch to a household from the inventory screen before reviewing reminders.
      </p>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
    </section>
  {:else}
    <section class="reminders-layout">
      <section class="panel reminders-panel" aria-labelledby="reminder-heading">
        <div class="section-heading">
          <div>
            <p class="eyebrow">{reminderMessages.headingEyebrow}</p>
            <h2 id="reminder-heading">{reminderMessages.headingTitle}</h2>
          </div>
          <div class="heading-actions">
            <span>{reminders.items.length}</span>
            <button
              class="secondary-action small"
              type="button"
              onclick={() => refreshReminders()}
              disabled={reminders.status === 'loading'}>Refresh</button
            >
          </div>
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
    </section>
  {/if}
</AppFrame>
