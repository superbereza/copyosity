<script lang="ts">
  import { onDestroy } from "svelte";
  import type { ClipboardEntry } from "$lib/types";
  import { copyEntry, activateEntry, deleteEntry, pinEntry, retagEntry } from "$lib/api";

  type ClickAction = "copy" | "paste" | "none";

  let {
    entry,
    selected = false,
    singleClickAction = "paste",
    doubleClickAction = "copy",
    onpasted,
    ondeleted,
    onpinned,
    onretagged,
  }: {
    entry: ClipboardEntry;
    selected?: boolean;
    singleClickAction?: ClickAction;
    doubleClickAction?: ClickAction;
    onpasted?: () => void;
    ondeleted?: () => void;
    onpinned?: () => void;
    onretagged?: () => void;
  } = $props();

  function timeAgo(dateStr: string): string {
    const now = Date.now();
    const then = new Date(dateStr).getTime();
    const diff = Math.floor((now - then) / 1000);

    if (diff < 60) return "just now";
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;
    return new Date(dateStr).toLocaleDateString();
  }

  function truncate(text: string, max: number): string {
    if (text.length <= max) return text;
    return text.slice(0, max) + "...";
  }

  function detectTextKind(text: string | null): string {
    if (!text) return "Text";

    const sample = text.trim();
    const lower = sample.toLowerCase();

    if (/^(https?:\/\/|www\.)/.test(lower)) return "URL";
    if (sample.length < 10000 && ((sample.startsWith("{") && sample.endsWith("}")) || (sample.startsWith("[") && sample.endsWith("]")))) {
      try {
        JSON.parse(sample);
        return "JSON";
      } catch {
        // fall through
      }
    }
    if (/^#!\/.*\b(bash|sh|zsh)\b/.test(lower)) return "Shell";
    if (/^(\$|#)\s+\S+/.test(sample) || /\b(curl|git|npm|pnpm|yarn|brew|ssh|docker|kubectl)\b/.test(lower)) {
      return "Bash";
    }
    if (/(^|\n)\s*(select|insert|update|delete|create table|alter table)\b/.test(lower)) return "SQL";
    if (/<[a-z][\s\S]*>/.test(lower)) return "HTML";
    if (/\b(function|const|let|import|export|=>)\b/.test(lower)) return "JavaScript";
    if (/\b(interface|type\s+\w+|implements|enum)\b/.test(lower)) return "TypeScript";
    if (/(^|\n)\s*(def |class |import |from .+ import )/.test(sample)) return "Python";
    if (/(^|\n)\s*(fn |let mut |impl |pub struct )/.test(sample)) return "Rust";

    return "Text";
  }

  let copied = $state(false);
  let clickTimer: ReturnType<typeof setTimeout> | undefined;
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  let clickable = $derived(entry.content_type === "text" || entry.content_type === "image");

  onDestroy(() => {
    clearTimeout(clickTimer);
    clearTimeout(copiedTimer);
  });

  function markCopied() {
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => {
      copied = false;
    }, 800);
  }

  async function runAction(action: ClickAction) {
    if (copied || !clickable || action === "none") return;
    if (action === "paste") {
      await activateEntry(entry.id);
      onpasted?.();
    } else if (action === "copy") {
      await copyEntry(entry.id);
      markCopied();
    }
  }

  function handleClick() {
    if (doubleClickAction === "none") {
      runAction(singleClickAction);
      return;
    }
    if (clickTimer) {
      clearTimeout(clickTimer);
      clickTimer = undefined;
      runAction(doubleClickAction);
      return;
    }
    clickTimer = setTimeout(() => {
      clickTimer = undefined;
      runAction(singleClickAction);
    }, 250);
  }

  async function handleActivate() {
    await runAction("paste");
  }

  async function handleCopy(e: MouseEvent) {
    e.stopPropagation();
    if (copied) return;
    await copyEntry(entry.id);
    markCopied();
  }

  async function handleDelete(e: MouseEvent) {
    e.stopPropagation();
    await deleteEntry(entry.id);
    ondeleted?.();
  }

  async function handlePin(e: MouseEvent) {
    e.stopPropagation();
    await pinEntry(entry.id, !entry.is_pinned);
    onpinned?.();
  }

  async function handleRetag(e: MouseEvent) {
    e.stopPropagation();
    await retagEntry(entry.id);
    onretagged?.();
  }

  let preview = $derived(entry.text_content ? truncate(entry.text_content, 200) : "");
  let textKind = $derived(detectTextKind(entry.text_content));
  let typeLabel = $derived(entry.content_type === "text" ? textKind : entry.content_type === "image" ? "Image" : "File");
  let charLabel = $derived(entry.char_count ? `${entry.char_count.toLocaleString()} characters` : "");
  let tags = $derived(entry.tags ?? []);
</script>

<div
  class="card"
  class:selected
  class:pinned={entry.is_pinned}
  class:copied
  onclick={handleClick}
  onkeydown={(e) => e.key === 'Enter' && handleActivate()}
  role="button"
  tabindex="0"
  title={entry.text_content ?? ""}
>
  <div class="card-header">
    <div class="card-type">
      <span class="type-label">{typeLabel}</span>
      <span class="time">{timeAgo(entry.created_at)}</span>
    </div>
    <div class="card-actions">
      <button class="action-btn" onclick={handleCopy} title="Copy">
        <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
          <rect x="5" y="5" width="9" height="9" rx="1.5" />
          <path d="M3 11V3a1 1 0 0 1 1-1h8" />
        </svg>
      </button>
      {#if entry.content_type === "text"}
        <button class="action-btn" onclick={handleRetag} title="Retag">
          <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
            <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" />
            <path d="M13.5 2v3h-3" />
          </svg>
        </button>
      {/if}
      <button class="action-btn" onclick={handlePin} title={entry.is_pinned ? "Unpin" : "Pin"}>
        <svg viewBox="0 0 16 16" width="12" height="12" fill={entry.is_pinned ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.2" stroke-linejoin="round">
          <path d="M8 1.5l1.96 4 4.4.65-3.18 3.1.75 4.38L8 11.55 4.07 13.62l.75-4.38L1.64 6.15l4.4-.65z" />
        </svg>
      </button>
      <button class="action-btn delete" onclick={handleDelete} title="Delete">
        <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
    </div>
  </div>

  <div class="card-body">
    {#if entry.content_type === "text"}
      <pre class="text-preview">{preview}</pre>
    {:else if entry.content_type === "image"}
      <div class="image-preview">
        {#if entry.image_thumb}
          <img src="data:image/png;base64,{entry.image_thumb}" alt="Copied content" loading="lazy" decoding="async" />
        {:else}
          <div class="image-placeholder">Image</div>
        {/if}
        <div class="image-meta">
          Image preview
        </div>
      </div>
    {/if}
  </div>

  <div class="card-footer">
    <div class="footer-meta">
      {#if entry.source_app}
        <span class="source-app">{entry.source_app}</span>
      {/if}
      {#if tags.length > 0}
        <div class="tags">
          {#each tags.slice(0, 3) as tag}
            <span class="tag-chip">{tag}</span>
          {/each}
        </div>
      {/if}
    </div>
    {#if charLabel}
      <span class="char-count">{charLabel}</span>
    {/if}
  </div>

  {#if copied}
    <div class="copied-overlay">
      <svg class="copied-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="20 6 9 17 4 12" />
      </svg>
      <span>Copied</span>
    </div>
  {/if}
</div>

<style>
  .card {
    position: relative;
    width: 220px;
    min-width: 220px;
    height: 280px;
    background: linear-gradient(180deg, rgba(58, 58, 66, 0.92), rgba(36, 36, 44, 0.88));
    border: 1px solid var(--border-default);
    border-radius: var(--radius-xl);
    padding: var(--space-3);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    transition: border-color 0.15s ease, background 0.15s ease, transform 0.15s ease, box-shadow 0.15s ease;
    font-family: inherit;
    color: inherit;
    text-align: left;
    overflow: hidden;
    flex-shrink: 0;
  }

  .card:hover {
    border-color: var(--accent-border);
    background: linear-gradient(180deg, rgba(66, 66, 76, 0.96), rgba(42, 42, 50, 0.92));
    transform: translateY(-2px);
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.28);
  }

  .card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-bg);
  }

  .card.pinned { border-color: rgba(217, 165, 90, 0.4); }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: var(--space-2);
    flex-shrink: 0;
  }

  .card-type {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .type-label {
    display: inline-flex;
    align-items: center;
    width: fit-content;
    padding: 2px var(--space-2);
    border-radius: 999px;
    background: var(--surface-2);
    font-weight: 500;
    font-size: var(--text-xs);
    letter-spacing: 0.02em;
    color: var(--fg-primary);
  }

  .time {
    font-size: var(--text-xs);
    color: var(--fg-muted);
  }

  .card-actions {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .card:hover .card-actions { opacity: 1; }

  .action-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.35);
    cursor: pointer;
    padding: 2px var(--space-1);
    border-radius: var(--radius-sm);
    line-height: 1;
    transition: background 0.15s ease, color 0.15s ease;
  }

  .action-btn:hover {
    color: var(--fg-primary);
    background: var(--surface-2);
  }

  .action-btn.delete:hover { color: var(--danger); }

  .card-body {
    flex: 1;
    overflow: hidden;
    margin-bottom: var(--space-2);
  }

  .text-preview {
    padding: var(--space-2) var(--space-3);
    background: var(--surface-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    line-height: var(--leading-body);
    color: var(--fg-primary);
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    font-family: "SF Mono", "Menlo", "Monaco", monospace;
    overflow: hidden;
    max-height: 100%;
  }

  .image-preview {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .image-preview img {
    width: 100%;
    height: 86px;
    border-radius: var(--radius-md);
    object-fit: cover;
    display: block;
    border: 1px solid var(--border-subtle);
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.22);
  }

  .image-placeholder {
    width: 100%;
    height: 86px;
    background: var(--surface-2);
    border-radius: var(--radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }

  .image-meta {
    padding: 6px var(--space-2);
    background: var(--surface-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--fg-secondary);
    font-size: var(--text-xs);
    line-height: var(--leading-body);
  }

  .card-footer {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  .footer-meta {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .source-app {
    font-size: var(--text-xs);
    color: var(--fg-muted);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tags {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .tag-chip {
    display: inline-flex;
    align-items: center;
    padding: 2px 6px;
    border-radius: 999px;
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    color: var(--fg-primary);
    font-size: 10px;
    font-weight: 500;
    line-height: 1.3;
    text-transform: lowercase;
  }

  .char-count {
    font-size: var(--text-xs);
    color: var(--fg-disabled);
  }

  .card.copied {
    border-color: var(--success);
    box-shadow: 0 0 0 2px var(--success-bg);
  }

  .copied-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    background: rgba(20, 20, 26, 0.88);
    backdrop-filter: blur(6px);
    border-radius: var(--radius-xl);
    color: var(--success);
    font-size: var(--text-lg);
    font-weight: 600;
    letter-spacing: 0.02em;
    animation: copied-pop 0.3s cubic-bezier(0.16, 1, 0.3, 1);
    z-index: 5;
  }

  .copied-icon {
    width: 28px;
    height: 28px;
    animation: check-draw 0.35s ease forwards;
  }

  @keyframes copied-pop {
    from {
      opacity: 0;
      transform: scale(0.9);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  @keyframes check-draw {
    from {
      stroke-dasharray: 40;
      stroke-dashoffset: 40;
    }
    to {
      stroke-dasharray: 40;
      stroke-dashoffset: 0;
    }
  }
</style>
