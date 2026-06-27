<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { ClickAction, ClipboardEntry, Collection } from "$lib/types";
  import {
    getEntries,
    getCollections,
    getAppSettings,
    hideMainWindow,
    openSettingsWindow,
    checkAccessibility,
  } from "$lib/api";
  import ClipboardCard from "$lib/components/ClipboardCard.svelte";
  import SearchBar from "$lib/components/SearchBar.svelte";
  import CollectionTabs from "$lib/components/CollectionTabs.svelte";

  let entries: ClipboardEntry[] = $state([]);
  let collections: Collection[] = $state([]);
  let searchQuery = $state("");
  let activeCollectionId: number | null = $state(null);
  let pinnedOnly = $state(false);
  let activeTag = $state<string | null>(null);
  let selectedIndex = $state(-1);
  let gridEl: HTMLDivElement | undefined = $state();
  let visible = $state(false);
  let revealCycle = $state(0);
  let singleClickAction = $state<ClickAction>("paste");
  let doubleClickAction = $state<ClickAction>("copy");
  let accessibilityGranted = $state<boolean | null>(null);
  const hiddenTopTags = new Set(["code", "otp", "token", "log"]);

  // Paste actions need Accessibility — without it activateEntry copies but
  // simulate_paste silently no-ops, looking like a broken click.
  let pasteWillFail = $derived(
    accessibilityGranted === false &&
      (singleClickAction === "paste" || doubleClickAction === "paste"),
  );

  async function loadBehaviorSettings() {
    try {
      const s = await getAppSettings();
      singleClickAction = (s.single_click_action as ClickAction) ?? "paste";
      doubleClickAction = (s.double_click_action as ClickAction) ?? "copy";
    } catch (e) {
      console.error("Failed to load behavior settings", e);
    }
  }

  async function refreshAccessibility() {
    try {
      accessibilityGranted = await checkAccessibility();
    } catch (e) {
      console.error("checkAccessibility failed", e);
    }
  }

  async function loadEntries() {
    entries = await getEntries({
      collection_id: activeCollectionId,
      pinned_only: pinnedOnly,
      search: searchQuery || null,
    });
  }

  async function loadCollections() {
    collections = await getCollections();
  }

  function showWindow() {
    window.getSelection()?.removeAllRanges();
    searchQuery = "";
    activeTag = null;
    selectedIndex = -1;
    loadEntries();
    revealCycle += 1;
    // Reset scroll to start
    if (gridEl) gridEl.scrollLeft = 0;
    // Start hidden, then animate in next frame
    visible = false;
    requestAnimationFrame(() => {
      visible = true;
    });
  }

  function animateOut() {
    visible = false;
    searchQuery = "";
    activeTag = null;
    selectedIndex = -1;
    hideMainWindow();
  }

  function forceHideWindow() {
    visible = false;
    searchQuery = "";
    activeTag = null;
    selectedIndex = -1;
    hideMainWindow();
  }

  onMount(() => {
    loadEntries();
    loadCollections();
    loadBehaviorSettings();
    refreshAccessibility();

    // Tell Rust we're loaded — it will hide the off-screen warmup window
    invoke("frontend_ready");

    // Debounce entry reloads — clipboard-changed and entry-tagged can fire together
    let reloadTimer: ReturnType<typeof setTimeout>;
    function scheduleReload() {
      clearTimeout(reloadTimer);
      reloadTimer = setTimeout(() => loadEntries(), 100);
    }

    const unlistenClipboard = listen("clipboard-changed", scheduleReload);
    const unlistenTagged = listen("entry-tagged", scheduleReload);

    const unlistenShow = listen("window-show", () => {
      showWindow();
      loadBehaviorSettings();
      refreshAccessibility();
    });

    const unlistenOpenSettings = listen("open-settings", () => {
      openSettingsWindow();
    });

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        forceHideWindow();
        return;
      }
      if (e.key === "ArrowRight") {
        e.preventDefault();
        selectedIndex = Math.min(selectedIndex + 1, filteredEntries.length - 1);
        scrollToSelected();
      }
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        selectedIndex = Math.max(selectedIndex - 1, 0);
        scrollToSelected();
      }
      if (e.key === "Enter" && selectedIndex >= 0 && selectedIndex < filteredEntries.length) {
        e.preventDefault();
        const entry = filteredEntries[selectedIndex];
        if (entry.text_content) {
          import("$lib/api").then(({ pasteEntry }) => {
            pasteEntry(entry.text_content!);
            animateOut();
          });
        }
      }
    };

    window.addEventListener("keydown", handleKeydown);

    return () => {
      clearTimeout(reloadTimer);
      clearTimeout(debounceTimer);
      unlistenClipboard.then((fn) => fn());
      unlistenTagged.then((fn) => fn());
      unlistenShow.then((fn) => fn());
      unlistenOpenSettings.then((fn) => fn());
      window.removeEventListener("keydown", handleKeydown);
    };
  });

  function scrollToSelected() {
    if (!gridEl) return;
    const cards = gridEl.querySelectorAll(".card");
    if (cards[selectedIndex]) {
      cards[selectedIndex].scrollIntoView({ behavior: "smooth", block: "nearest", inline: "center" });
    }
  }

  function handleSearch(q: string) {
    searchQuery = q;
    selectedIndex = -1;
    loadEntries();
  }

  function handleCollectionSelect(id: number | null) {
    pinnedOnly = id === -1;
    activeCollectionId = id === -1 ? null : id;
    activeTag = null;
    selectedIndex = -1;
    loadEntries();
  }

  function handleEntryAction() {
    loadEntries();
  }

  function handlePasted() {
    animateOut();
  }

  let debounceTimer: ReturnType<typeof setTimeout>;
  function debouncedSearch(q: string) {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => handleSearch(q), 150);
  }

  let topTags = $derived.by(() => {
    const counts = new Map<string, number>();

    for (const entry of entries) {
      for (const tag of entry.tags ?? []) {
        if (hiddenTopTags.has(tag)) continue;
        counts.set(tag, (counts.get(tag) ?? 0) + 1);
      }
    }

    return [...counts.entries()]
      .sort((a, b) => {
        if (b[1] !== a[1]) return b[1] - a[1];
        return a[0].localeCompare(b[0]);
      })
      .slice(0, 8);
  });

  let filteredEntries = $derived.by(() => {
    if (!activeTag) return entries;
    const tag = activeTag;
    return entries.filter((entry) => (entry.tags ?? []).includes(tag));
  });
</script>

<div class="app" class:visible>
  {#if pasteWillFail}
    <button class="a11y-banner" type="button" onclick={() => openSettingsWindow()}>
      <span class="a11y-banner-dot"></span>
      <span class="a11y-banner-text">
        Paste won't work — Accessibility permission required. Click to fix.
      </span>
    </button>
  {/if}
  <header class="header">
    <SearchBar value={searchQuery} onchange={debouncedSearch} />
    <CollectionTabs
      {collections}
      activeId={activeCollectionId}
      activePinned={pinnedOnly}
      onselect={handleCollectionSelect}
      onupdate={loadCollections}
    />
    <div class="header-actions">
      <button
        class="settings-btn"
        type="button"
        aria-label="Open settings"
        onclick={() => openSettingsWindow()}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M19.14 12.94c.04-.31.06-.62.06-.94s-.02-.63-.06-.94l2.03-1.58a.5.5 0 0 0 .12-.64l-1.92-3.32a.5.5 0 0 0-.6-.22l-2.39.96a7.03 7.03 0 0 0-1.63-.94l-.36-2.54a.5.5 0 0 0-.5-.42h-3.84a.5.5 0 0 0-.5.42l-.36 2.54c-.58.22-1.13.53-1.63.94l-2.39-.96a.5.5 0 0 0-.6.22L2.71 8.84a.5.5 0 0 0 .12.64l2.03 1.58c-.04.31-.06.62-.06.94s.02.63.06.94l-2.03 1.58a.5.5 0 0 0-.12.64l1.92 3.32a.5.5 0 0 0 .6.22l2.39-.96c.5.41 1.05.72 1.63.94l.36 2.54a.5.5 0 0 0 .5.42h3.84a.5.5 0 0 0 .5-.42l.36-2.54c.58-.22 1.13-.53 1.63-.94l2.39.96a.5.5 0 0 0 .6-.22l1.92-3.32a.5.5 0 0 0-.12-.64zM12 15.5A3.5 3.5 0 1 1 12 8.5a3.5 3.5 0 0 1 0 7z"
          />
        </svg>
      </button>
    </div>
  </header>

  {#if topTags.length > 0}
    <div class="tag-groups">
      <button
        class="tag-group-chip"
        class:active={!activeTag}
        type="button"
        onclick={() => {
          activeTag = null;
          selectedIndex = -1;
        }}
      >
        All tags
      </button>

      {#each topTags as [tag, count]}
        <button
          class="tag-group-chip"
          class:active={activeTag === tag}
          type="button"
          onclick={() => {
            activeTag = tag;
            selectedIndex = -1;
          }}
        >
          <span>{tag}</span>
          <span class="tag-group-count">{count}</span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="grid-container" bind:this={gridEl}>
    {#if filteredEntries.length === 0}
      <div class="empty-state">
        {#if searchQuery || activeTag}
          <p>No results for "{searchQuery}"</p>
        {:else}
          <p>Clipboard history is empty</p>
          <p class="hint">Copy something to get started</p>
        {/if}
      </div>
    {:else}
      {#each filteredEntries as entry, i (`${revealCycle}-${activeTag ?? 'all'}-${entry.id}`)}
        <div class="card-wrapper" style="animation-delay: {Math.min(i * 30, 300)}ms">
          <ClipboardCard
            {entry}
            selected={i === selectedIndex}
            {singleClickAction}
            {doubleClickAction}
            onpasted={handlePasted}
            ondeleted={handleEntryAction}
            onpinned={handleEntryAction}
          />
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent;
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
    font-size: var(--text-md);
    color: var(--fg-primary);
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(*) {
    box-sizing: border-box;
    outline: none;
  }

  :global(::selection) {
    background: transparent;
  }

  .app {
    width: 100vw;
    height: 100vh;
    background:
      linear-gradient(180deg, rgba(36, 36, 42, 0.94), rgba(20, 20, 26, 0.90));
    backdrop-filter: blur(34px) saturate(1.15);
    -webkit-backdrop-filter: blur(34px) saturate(1.15);
    border-radius: 18px;
    border: 1px solid var(--border-strong);
    box-shadow:
      0 18px 50px rgba(0, 0, 0, 0.28),
      inset 0 1px 0 rgba(255, 255, 255, 0.08);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transform: translateY(26px) scale(0.985);
    opacity: 0;
    transition:
      transform 0.24s cubic-bezier(0.22, 1, 0.36, 1),
      opacity 0.22s ease;
  }

  .app.visible {
    transform: translateY(0) scale(1);
    opacity: 1;
  }

  .header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .a11y-banner {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: 6px var(--space-4);
    background: #2a2218;
    border: none;
    border-bottom: 1px solid #4a3a22;
    color: #e8c47a;
    font: inherit;
    font-size: var(--text-xs);
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    flex-shrink: 0;
    transition: background 0.15s ease;
  }

  .a11y-banner:hover { background: #322a1e; }
  .a11y-banner:active { background: #221a12; }

  .a11y-banner-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #d9a55a;
    flex-shrink: 0;
  }

  .a11y-banner-text { flex: 1; }

  .tag-groups {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4) 0;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tag-groups::-webkit-scrollbar { display: none; }

  .tag-group-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: 5px var(--space-3);
    border-radius: 999px;
    border: 1px solid var(--border-subtle);
    background: var(--surface-1);
    color: var(--fg-secondary);
    cursor: pointer;
    white-space: nowrap;
    font: inherit;
    font-size: var(--text-xs);
    font-weight: 500;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }

  .tag-group-chip:hover {
    background: var(--surface-2);
    color: var(--fg-primary);
  }

  .tag-group-chip.active {
    background: var(--accent-bg-strong);
    border-color: var(--accent-border);
    color: var(--fg-primary);
  }

  .tag-group-count {
    display: inline-flex;
    min-width: 16px;
    justify-content: center;
    padding: 1px 4px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    font-size: 10px;
    font-weight: 500;
    line-height: 1.2;
    color: var(--fg-secondary);
  }

  .header-actions {
    position: relative;
    margin-left: auto;
    flex-shrink: 0;
  }

  .settings-btn {
    width: 30px;
    height: 30px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--fg-secondary);
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease;
  }

  .settings-btn:hover {
    background: var(--surface-3);
    color: var(--fg-primary);
  }

  .settings-btn svg {
    width: 16px;
    height: 16px;
    fill: currentColor;
  }

  .grid-container {
    flex: 1;
    display: flex;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-4);
    overflow-x: auto;
    overflow-y: hidden;
    align-items: flex-start;
    scrollbar-width: thin;
    scrollbar-color: rgba(255, 255, 255, 0.1) transparent;
  }

  .grid-container::-webkit-scrollbar {
    height: 6px;
  }

  .grid-container::-webkit-scrollbar-track {
    background: transparent;
  }

  .grid-container::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.1);
    border-radius: 3px;
  }

  .card-wrapper {
    animation: card-enter 0.35s cubic-bezier(0.16, 1, 0.3, 1) backwards;
  }

  @keyframes card-enter {
    from {
      opacity: 0;
      transform: translateY(20px) scale(0.95);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .empty-state {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }

  .empty-state p {
    margin: var(--space-1) 0;
  }

  .hint {
    font-size: var(--text-xs);
    color: var(--fg-disabled);
  }
</style>
