<script lang="ts">
  import { page } from '$app/state';
  import type { Snippet } from 'svelte';
  import { appPath } from '$lib/paths';
  import type { HouseholdSummary } from '$lib/session-core';

  type ActiveSection = 'inventory' | 'products' | 'reminders' | 'settings';

  interface NavItem {
    key: ActiveSection;
    label: string;
    href: string;
  }

  let {
    title,
    eyebrow = 'Quartermaster',
    authenticated = false,
    active,
    activeHousehold = null,
    households = [],
    onhouseholdchange,
    onimportbackup,
    onlogout,
    children
  }: {
    title: string;
    eyebrow?: string;
    authenticated?: boolean;
    active?: ActiveSection;
    activeHousehold?: HouseholdSummary | null;
    households?: HouseholdSummary[];
    onhouseholdchange?: (householdId: string) => void | Promise<void>;
    onimportbackup?: () => void | Promise<void>;
    onlogout?: () => void | Promise<void>;
    children?: Snippet;
  } = $props();

  const brandMarkSrc = $derived(appPath('/brand/quartermaster-mark.svg', page.url));
  const navItems = $derived<NavItem[]>([
    { key: 'inventory', label: 'Inventory', href: appPath('/', page.url) },
    { key: 'products', label: 'Products', href: appPath('/products', page.url) },
    { key: 'reminders', label: 'Reminders', href: appPath('/reminders', page.url) },
    { key: 'settings', label: 'Settings', href: appPath('/settings', page.url) }
  ]);
</script>

<main class="app-shell">
  <header class="topbar">
    <div class="brand-heading">
      <img class="brand-mark" src={brandMarkSrc} alt="" />
      <div>
        <p class="eyebrow">{eyebrow}</p>
        <h1>{title}</h1>
      </div>
    </div>
    {#if authenticated}
      <div class="app-nav-region">
        {#if activeHousehold}
          <div class="household-switcher">
            <span class="eyebrow">Household</span>
            {#if households.length > 1 && onhouseholdchange}
              <select
                aria-label="Current household"
                onchange={(event) => onhouseholdchange(event.currentTarget.value)}
                value={activeHousehold.id}
              >
                {#each households as household}
                  <option value={household.id}>{household.name}</option>
                {/each}
              </select>
            {:else}
              <strong>{activeHousehold.name}</strong>
            {/if}
          </div>
        {/if}
        {#if onimportbackup}
          <button class="ghost-button" type="button" onclick={onimportbackup}>Import backup</button>
        {/if}
        <nav class="app-nav" aria-label="Primary">
          {#each navItems as item}
            <a
              href={item.href}
              class:active={active === item.key}
              aria-current={active === item.key ? 'page' : undefined}>{item.label}</a
            >
          {/each}
        </nav>
        {#if onlogout}
          <button class="ghost-button" type="button" onclick={onlogout}>Log out</button>
        {/if}
      </div>
    {/if}
  </header>

  {@render children?.()}
</main>
