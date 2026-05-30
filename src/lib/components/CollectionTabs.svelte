<script lang="ts">
  import type { Collection } from "$lib/types";
  import { createCollection, deleteCollection } from "$lib/api";

  let {
    collections = [],
    activeId = null,
    activePinned = false,
    onselect,
    onupdate,
  }: {
    collections?: Collection[];
    activeId?: number | null;
    activePinned?: boolean;
    onselect?: (id: number | null) => void;
    onupdate?: () => void;
  } = $props();

  let showAdd = $state(false);
  let newName = $state("");

  async function handleAdd() {
    if (!newName.trim()) return;
    await createCollection(newName.trim());
    newName = "";
    showAdd = false;
    onupdate?.();
  }

  async function handleDelete(e: MouseEvent, id: number) {
    e.stopPropagation();
    await deleteCollection(id);
    if (activeId === id) onselect?.(null);
    onupdate?.();
  }
</script>

<div class="tabs-container">
  <button
    class="tab"
    class:active={activeId === null && !activePinned}
    onclick={() => onselect?.(null)}
  >
    Clipboard History
  </button>

  <button
    class="tab"
    class:active={activePinned}
    onclick={() => onselect?.(-1)}
  >
    Starred
  </button>

  {#each collections as col}
    <div
      class="tab"
      class:active={activeId === col.id && !activePinned}
      onclick={() => onselect?.(col.id)}
      onkeydown={(e) => e.key === 'Enter' && onselect?.(col.id)}
      role="button"
      tabindex="0"
    >
      <span class="tab-dot" style:background={col.color ?? '#666'}></span>
      {col.name}
      <button class="tab-delete" onclick={(e) => handleDelete(e, col.id)}>×</button>
    </div>
  {/each}

  {#if showAdd}
    <form class="add-form" onsubmit={(e) => { e.preventDefault(); handleAdd(); }}>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:value={newName}
        placeholder="Name..."
        autofocus
        onblur={() => { if (!newName) showAdd = false; }}
      />
    </form>
  {:else}
    <button class="tab add-tab" onclick={() => (showAdd = true)}>+</button>
  {/if}
</div>

<style>
  .tabs-container {
    display: flex;
    align-items: center;
    gap: 2px;
    overflow-x: auto;
    padding: 0 var(--space-1);
    scrollbar-width: none;
  }

  .tabs-container::-webkit-scrollbar { display: none; }

  .tab {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px var(--space-3);
    border-radius: var(--radius-sm);
    background: none;
    border: none;
    color: var(--fg-muted);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    font-family: inherit;
    transition: background 0.15s ease, color 0.15s ease;
  }

  .tab:hover {
    color: var(--fg-primary);
    background: var(--surface-2);
  }

  .tab.active {
    color: var(--fg-primary);
    background: var(--surface-3);
  }

  .tab-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tab-delete {
    background: none;
    border: none;
    color: var(--fg-disabled);
    cursor: pointer;
    font-size: var(--text-md);
    padding: 0 2px;
    line-height: 1;
    opacity: 0;
    transition: opacity 0.15s, color 0.15s;
  }

  .tab:hover .tab-delete { opacity: 1; }
  .tab-delete:hover { color: var(--danger); }

  .add-tab {
    font-size: var(--text-md);
    color: var(--fg-muted);
  }

  .add-form input {
    background: var(--surface-2);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-sm);
    color: var(--fg-primary);
    padding: 4px var(--space-2);
    font-size: var(--text-sm);
    outline: none;
    width: 120px;
    font-family: inherit;
  }

  .add-form input:focus {
    border-color: var(--accent-border);
    box-shadow: 0 0 0 3px var(--accent-bg);
  }
</style>
