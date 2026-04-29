<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import { appPath } from '$lib/paths';

  onMount(() => {
    const id = page.params.id ?? '';
    void goto(`${appPath('/', page.url)}?batch=${encodeURIComponent(id)}`, { replaceState: true });
  });
</script>

<main class="app-shell">
  <section class="panel empty-state">
    <p class="muted">Opening batch...</p>
  </section>
</main>
