<script lang="ts">
  import { page } from '$app/state';
  import { quartermasterJoinUrl, readJoinDetails } from '$lib/join';

  const details = $derived(readJoinDetails(page.url.searchParams));
  const deepLink = $derived(quartermasterJoinUrl(details));
</script>

<svelte:head>
  <title>Join Quartermaster</title>
</svelte:head>

<main class="join-shell">
  <section class="join-panel">
    <p class="eyebrow">Household invite</p>
    <h1>Join Quartermaster</h1>
    <p class="lede">Open the app to join a household with the invite details below.</p>
    <a class="primary-action" href={deepLink}>Open in Quartermaster</a>

    <div class="detail-grid">
      <div>
        <h2>Invite code</h2>
        <code>{details.invite || 'No invite code provided'}</code>
      </div>
      <div>
        <h2>Server URL</h2>
        <code>{details.server || 'Use your app default'}</code>
      </div>
    </div>

    <p class="muted">
      If the app does not open automatically, copy the invite code and server URL into Quartermaster
      manually.
    </p>
  </section>
</main>
