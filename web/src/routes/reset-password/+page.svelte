<script lang="ts">
  import { browser } from '$app/environment';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  import { generatedTransport } from '$lib/api';
  import { createBrowserSessionStorage, QuartermasterSession } from '$lib/session-core';

  let session: QuartermasterSession | null = $state(null);
  let email = $state('');
  let token = $state('');
  let code = $state('');
  let newPassword = $state('');
  let busy = $state(false);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);

  onMount(() => {
    if (!browser) {
      return;
    }
    session = new QuartermasterSession(
      createBrowserSessionStorage(window.localStorage, window.location),
      generatedTransport()
    );
    email = page.url.searchParams.get('email') ?? '';
    token = page.url.searchParams.get('token') ?? '';
  });

  async function submit() {
    if (!session) {
      return;
    }
    busy = true;
    message = null;
    error = null;
    try {
      await session.confirmPasswordReset(email, newPassword, {
        token: token.trim() || null,
        code: code.trim() || null
      });
      message = 'Password reset. You can now log in with your new password.';
      newPassword = '';
      code = '';
      token = '';
    } catch {
      error = 'Password reset failed.';
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head>
  <title>Reset password - Quartermaster</title>
</svelte:head>

<main class="auth-layout">
  <form
    class="panel auth-panel"
    onsubmit={(event) => {
      event.preventDefault();
      void submit();
    }}
  >
    <h1>Reset password</h1>
    <label>
      Email
      <input bind:value={email} type="email" autocomplete="email" required />
    </label>
    {#if !token}
      <label>
        Reset code
        <input bind:value={code} autocomplete="one-time-code" required />
      </label>
    {/if}
    <label>
      New password
      <input
        bind:value={newPassword}
        type="password"
        autocomplete="new-password"
        minlength="8"
        required
      />
    </label>
    {#if message}
      <p class="success-text">{message}</p>
    {/if}
    {#if error}
      <p class="error-text">{error}</p>
    {/if}
    <button
      class="primary-action"
      type="submit"
      disabled={busy || !email || newPassword.length < 8 || (!token && !code)}
    >
      {busy ? 'Working...' : 'Reset password'}
    </button>
  </form>
</main>
