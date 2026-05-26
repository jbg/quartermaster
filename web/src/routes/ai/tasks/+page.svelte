<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import AppFrame from '$lib/components/AppFrame.svelte';
  import { generatedTransport } from '$lib/api';
  import { aiTaskList, aiTaskStateUpdate } from '$lib/generated/sdk.gen';
  import type { AiTaskDto, AiTaskUserState } from '$lib/generated/types.gen';
  import { jsonPreview, unwrapGenerated } from '$lib/phase8';
  import { appPath } from '$lib/paths';
  import {
    currentHousehold,
    createBrowserSessionStorage,
    QuartermasterSession,
    type MeResponse
  } from '$lib/session-core';

  let session: QuartermasterSession | null = $state(null);
  let me = $state<MeResponse | null>(null);
  let tasks = $state<AiTaskDto[]>([]);
  let authenticated = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);

  const activeHousehold = $derived(me ? currentHousehold(me) : null);
  const households = $derived(me?.households ?? []);
  const inventoryHref = $derived(appPath('/', page.url));
  const cartReviewHref = $derived(appPath('/suppliers/review', page.url));

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
    void loadTasks();
  });

  async function loadTasks() {
    if (!session) {
      return;
    }
    loading = true;
    error = null;
    try {
      me = await session.me();
      tasks = unwrapGenerated(await aiTaskList({ query: { limit: 50 } })).items;
    } catch (err) {
      authenticated = false;
      error = err instanceof Error ? err.message : 'AI tasks could not be loaded.';
    } finally {
      loading = false;
    }
  }

  async function setTaskState(task: AiTaskDto, userState: AiTaskUserState) {
    try {
      const updated = unwrapGenerated(
        await aiTaskStateUpdate({ path: { id: task.id }, body: { user_state: userState } })
      );
      tasks = tasks.map((item) => (item.id === updated.id ? updated : item));
    } catch (err) {
      error = err instanceof Error ? err.message : 'AI task state could not be updated.';
    }
  }

  async function switchHousehold(id: string) {
    if (!session) {
      return;
    }
    me = await session.switchHousehold(id);
    await loadTasks();
  }

  async function logout() {
    if (!session) {
      return;
    }
    await session.logout();
    authenticated = false;
    me = null;
    await goto(inventoryHref);
  }
</script>

<svelte:head>
  <title>AI Tasks · Quartermaster</title>
</svelte:head>

<AppFrame
  title="AI Tasks"
  eyebrow="Automation"
  {authenticated}
  active="automation"
  {activeHousehold}
  {households}
  onhouseholdchange={switchHousehold}
  onlogout={logout}
>
  {#if loading}
    <section class="panel empty-state"><p class="muted">Loading AI tasks...</p></section>
  {:else if !authenticated}
    <section class="panel empty-state">
      <h2>Sign in required</h2>
      <a class="primary-action" href={inventoryHref}>Go to inventory</a>
      {#if error}<p class="error-text">{error}</p>{/if}
    </section>
  {:else}
    <section class="panel">
      <div class="section-heading">
        <div>
          <p class="eyebrow">Debugging</p>
          <h2>Task audit</h2>
        </div>
        <a class="secondary-action small" href={cartReviewHref}>Cart review</a>
      </div>
      {#if tasks.length === 0}
        <p class="muted">No AI tasks recorded for this household.</p>
      {:else}
        <div class="task-list">
          {#each tasks as task}
            <article class="task-row" data-testid={`ai-task-row-${task.id}`}>
              <div class="section-heading">
                <div>
                  <h3>{task.task_type.replaceAll('_', ' ')}</h3>
                  <p class="muted">
                    {task.provider} · {task.validation_status} · {task.user_state}
                  </p>
                </div>
                <select
                  value={task.user_state}
                  onchange={(event) =>
                    void setTaskState(task, event.currentTarget.value as AiTaskUserState)}
                  data-testid={`ai-task-state-${task.id}`}
                >
                  <option value="proposed">Proposed</option>
                  <option value="accepted">Accepted</option>
                  <option value="edited">Edited</option>
                  <option value="rejected">Rejected</option>
                </select>
              </div>
              <pre>{jsonPreview(task.input_summary)}</pre>
              {#if task.validation_errors.length > 0}
                <p class="error-text">{task.validation_errors.join(', ')}</p>
              {/if}
            </article>
          {/each}
        </div>
      {/if}
      {#if error}<p class="error-text">{error}</p>{/if}
    </section>
  {/if}
</AppFrame>

<style>
  .task-list {
    display: grid;
    gap: 0.75rem;
  }

  .task-row {
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 0.85rem;
  }

  .task-row h3 {
    margin: 0;
  }

  pre {
    overflow: auto;
    white-space: pre-wrap;
  }
</style>
