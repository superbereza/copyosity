<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { AppSettings, ExcludedApp, ModelCatalog, ModelOption } from "$lib/types";
  import {
    addExcludedApp,
    addFrontmostAppToExcluded,
    clearHistory,
    getAppSettings,
    getExcludedApps,
    getModelCatalog,
    rebindMainShortcut,
    restartAppWithSettingsOpen,
    checkForUpdate,
    type UpdateCheckResult,
    removeExcludedApp,
    updateAppSettings,
    checkAccessibility,
    requestAccessibility,
    checkOllamaStatus,
    unloadOllamaModel,
    startOllamaServer,
    pullOllamaModel,
    testOllamaTagging,
    type OllamaStatus,
  } from "$lib/api";
  import { openUrl } from "@tauri-apps/plugin-opener";

  type TabId = "general" | "behavior" | "ai" | "privacy" | "permissions";

  const tabs: { id: TabId; label: string }[] = [
    { id: "general", label: "General" },
    { id: "behavior", label: "Behavior" },
    { id: "ai", label: "AI & Tags" },
    { id: "privacy", label: "Privacy" },
    { id: "permissions", label: "Permissions" },
  ];

  let activeTab = $state<TabId>("general");

  let settings = $state<AppSettings>({
    ollama_model: "qwen3:4b-instruct-2507-q4_K_M",
    retention_days: 30,
    main_shortcut: "cmd+shift+v",
    show_in_dock: false,
    single_click_action: "paste",
    double_click_action: "copy",
  });
  let modelCatalog = $state<ModelCatalog>({
    total_memory_gb: 0,
    recommended_memory_gb: 0,
    options: [],
  });
  let selectedModelPreset = $state("__custom__");
  let excludedApps: ExcludedApp[] = $state([]);
  let excludedAppInput = $state("");
  let savingSettings = $state(false);
  let settingsNotice = $state("");
  let savedModel = $state("");
  let savedShowInDock = $state<boolean | null>(null);
  let restartRequired = $state(false);

  type UpdateState =
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "up-to-date"; current: string }
    | { kind: "available"; current: string; latest: string; url: string }
    | { kind: "error"; message: string };

  let updateState = $state<UpdateState>({ kind: "idle" });

  async function handleCheckForUpdate() {
    updateState = { kind: "checking" };
    try {
      const r: UpdateCheckResult = await checkForUpdate();
      if (r.has_update) {
        updateState = { kind: "available", current: r.current, latest: r.latest, url: r.release_url };
      } else {
        updateState = { kind: "up-to-date", current: r.current };
      }
    } catch (e) {
      updateState = { kind: "error", message: String(e) };
    }
  }

  let accessibilityGranted = $state<boolean | null>(null);

  let ollamaStatus = $state<OllamaStatus | null>(null);
  let ollamaLoading = $state(false);
  let pullProgress = $state("");
  let taggingResult = $state<string[] | null | undefined>(undefined);
  let taggingLoading = $state(false);

  const retentionOptions = [
    { label: "1 day", value: 1 },
    { label: "1 week", value: 7 },
    { label: "1 month", value: 30 },
    { label: "6 months", value: 180 },
  ];

  async function loadSettings() {
    settings = await getAppSettings();
    selectedModelPreset = settings.ollama_model;
    savedModel = settings.ollama_model;
    savedShowInDock = settings.show_in_dock;
  }

  async function loadModelCatalog() {
    modelCatalog = await getModelCatalog();
    if (!modelCatalog.options.some((o) => o.value === settings.ollama_model)) {
      selectedModelPreset = "__custom__";
    }
  }

  async function loadExcludedApps() {
    excludedApps = await getExcludedApps();
  }

  async function refreshOllamaStatus() {
    ollamaLoading = true;
    taggingResult = undefined;
    try {
      ollamaStatus = await checkOllamaStatus();
    } finally {
      ollamaLoading = false;
    }
  }

  async function handleStartServer() {
    ollamaLoading = true;
    try {
      await startOllamaServer();
      await refreshOllamaStatus();
    } finally {
      ollamaLoading = false;
    }
  }

  async function handlePullModel() {
    ollamaLoading = true;
    pullProgress = "Starting download...";
    await pullOllamaModel();
    // Command returns immediately, progress comes via events
    // ollama-pull-done will reset the state
  }

  async function handleTestTagging() {
    taggingLoading = true;
    taggingResult = undefined;
    try {
      taggingResult = await testOllamaTagging();
    } finally {
      taggingLoading = false;
    }
  }

  onMount(() => {
    // Load everything in parallel instead of sequentially
    loadSettings();
    loadModelCatalog();
    loadExcludedApps();
    refreshOllamaStatus();
    checkAccessibility().then((v) => (accessibilityGranted = v));

    const unlistenPull = listen<string>("ollama-pull-progress", (event) => {
      pullProgress = event.payload;
    });

    const unlistenPullDone = listen<boolean>("ollama-pull-done", async (event) => {
      ollamaLoading = false;
      pullProgress = "";
      await refreshOllamaStatus();
    });

    const unlistenUpdateCheck = listen("trigger-update-check", () => {
      activeTab = "general";
      handleCheckForUpdate();
    });

    const unlistenShowPermissions = listen("show-permissions", () => {
      activeTab = "permissions";
    });

    // Auto-poll accessibility while not granted — user may grant in System
    // Settings while this window is open and shouldn't need to restart or
    // click Recheck. Stops polling once granted; next Settings open re-checks.
    let accessibilityTimer: ReturnType<typeof setInterval> | undefined;
    accessibilityTimer = setInterval(async () => {
      const next = await checkAccessibility();
      if (next !== accessibilityGranted) {
        accessibilityGranted = next;
      }
      if (next === true) {
        clearInterval(accessibilityTimer);
        accessibilityTimer = undefined;
      }
    }, 2000);

    return () => {
      unlistenPull.then((fn) => fn());
      unlistenPullDone.then((fn) => fn());
      unlistenUpdateCheck.then((fn) => fn());
      unlistenShowPermissions.then((fn) => fn());
      clearInterval(accessibilityTimer);
    };
  });

  // Click-to-copy for shell commands shown inside .settings-hint / .status-hint
  let copiedCmd = $state<string | null>(null);
  async function copyCmd(cmd: string) {
    try {
      await navigator.clipboard.writeText(cmd);
      copiedCmd = cmd;
      setTimeout(() => {
        if (copiedCmd === cmd) copiedCmd = null;
      }, 1200);
    } catch (e) {
      console.error("copy failed", e);
    }
  }

  async function saveSettings() {
    savingSettings = true;
    settingsNotice = "";
    try {
      settings = await updateAppSettings({
        ollama_model: settings.ollama_model,
        retention_days: settings.retention_days,
        main_shortcut: settings.main_shortcut,
        show_in_dock: settings.show_in_dock,
        single_click_action: settings.single_click_action,
        double_click_action: settings.double_click_action,
      });
      const dockChanged = savedShowInDock !== null && savedShowInDock !== settings.show_in_dock;
      savedModel = settings.ollama_model;
      settingsNotice = dockChanged ? "Saved — restart required for Dock change" : "Saved";
      restartRequired = dockChanged;
      savedShowInDock = settings.show_in_dock;
      taggingResult = undefined;
      await Promise.all([
        rebindMainShortcut(),
        loadModelCatalog(),
        refreshOllamaStatus(),
      ]);
    } finally {
      savingSettings = false;
    }
  }

  function handleModelPresetChange(value: string) {
    selectedModelPreset = value;
    if (value !== "__custom__") {
      settings.ollama_model = value;
    }
  }

  async function handleAddExcludedApp() {
    const value = excludedAppInput.trim();
    if (!value) return;
    await addExcludedApp(value);
    excludedAppInput = "";
    await loadExcludedApps();
  }

  async function handleAddFrontmostApp() {
    const added = await addFrontmostAppToExcluded();
    settingsNotice = added ? `Excluded ${added}` : "No active app detected";
    await loadExcludedApps();
  }

  async function handleRemoveExcludedApp(id: number) {
    await removeExcludedApp(id);
    await loadExcludedApps();
  }

  async function handleClearHistory() {
    await clearHistory();
    settingsNotice = "History cleared";
  }

  let selectedModelMeta = $derived.by<ModelOption | null>(() => {
    return modelCatalog.options.find((o) => o.value === settings.ollama_model) ?? null;
  });

  let modelDirty = $derived(settings.ollama_model !== savedModel);

  // data-tauri-drag-region is unreliable on NSPanel (and steals clicks from
  // any interactive descendant). Explicit startDragging() on mousedown is the
  // recommended pattern: only fires when the press is outside an interactive
  // element, so buttons/inputs keep their clicks.
  //
  // <label> is intentionally NOT in the blocklist: our settings rows wrap the
  // title/input/hint in a wide <label>, so blocking it would kill drag across
  // half the window. Click directly on the input to focus.
  const DRAG_BLOCKLIST = "button, input, select, textarea, a, [contenteditable]";
  function startDrag(e: MouseEvent) {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest(DRAG_BLOCKLIST)) return;
    getCurrentWindow().startDragging();
  }
</script>

<div class="settings-page" onmousedown={startDrag} role="presentation">

  {#if restartRequired}
    <div class="restart-banner">
      <div class="restart-banner-text">Restart Copyosity to apply Dock change.</div>
      <button class="restart-banner-btn" type="button" onclick={() => restartAppWithSettingsOpen()}>
        Restart
      </button>
    </div>
  {/if}

  <nav class="settings-tabs" aria-label="Settings sections">
    {#each tabs as tab}
      <button
        type="button"
        class="settings-tab"
        class:active={activeTab === tab.id}
        onclick={() => (activeTab = tab.id)}
      >
        {tab.label}
      </button>
    {/each}
  </nav>

  {#if activeTab === "general"}
    <section class="settings-section">
      <div class="settings-section-title">Main Shortcut</div>
      <label class="settings-field">
        <span class="settings-label">Open / close clipboard history</span>
        <input
          class="settings-input"
          type="text"
          bind:value={settings.main_shortcut}
          placeholder="cmd+shift+v"
        />
        <div class="settings-hint">
          Use: <code>cmd</code>, <code>option</code>, <code>ctrl</code>, <code>shift</code> + key.
          Examples: <code>cmd+shift+v</code>, <code>ctrl+space</code>, <code>option+v</code>
        </div>
      </label>
    </section>

    <section class="settings-section">
      <div class="settings-section-title">Dock</div>
      <label class="settings-toggle">
        <input type="checkbox" bind:checked={settings.show_in_dock} />
        <span class="settings-toggle-label">Show in Dock</span>
      </label>
      <div class="settings-hint">
        When off, Copyosity runs as a macOS Accessory app — visible only in the menu bar.
        Changing this requires an app restart; you'll be prompted after saving.
      </div>
    </section>

    <section class="settings-section">
      <div class="settings-section-title">Storage</div>
      <label class="settings-field">
        <span class="settings-label">History retention</span>
        <select class="settings-select" bind:value={settings.retention_days}>
          {#each retentionOptions as option}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
    </section>

    <section class="settings-section">
      <div class="settings-section-title">Updates</div>
      <div class="update-row">
        {#if updateState.kind === "checking"}
          <span class="spinner"></span>
          <span class="update-status">Checking for updates…</span>
        {:else if updateState.kind === "up-to-date"}
          <span class="status-dot ok"></span>
          <span class="update-status">You're up to date · v{updateState.current}</span>
        {:else if updateState.kind === "available"}
          <span class="status-dot fail"></span>
          <span class="update-status">v{updateState.latest} available · current v{updateState.current}</span>
          <button class="settings-small-btn" type="button" onclick={() => openUrl(updateState.kind === "available" ? updateState.url : "")}>
            Open release
          </button>
        {:else if updateState.kind === "error"}
          <span class="status-dot fail"></span>
          <span class="update-status update-error">{updateState.message}</span>
        {:else}
          <span class="update-status update-muted">Click to check the latest release</span>
        {/if}
        <button class="settings-ghost-btn update-check-btn" type="button" disabled={updateState.kind === "checking"} onclick={handleCheckForUpdate}>
          Check now
        </button>
      </div>
    </section>
  {/if}

  {#if activeTab === "behavior"}
    <section class="settings-section">
      <div class="settings-section-title">Click Behavior</div>
      <label class="settings-field">
        <span class="settings-label">Single click on card</span>
        <select class="settings-select" bind:value={settings.single_click_action}>
          <option value="paste">Paste &amp; close window</option>
          <option value="copy">Copy to clipboard</option>
        </select>
      </label>
      <label class="settings-field">
        <span class="settings-label">Double click on card</span>
        <select class="settings-select" bind:value={settings.double_click_action}>
          <option value="copy">Copy to clipboard</option>
          <option value="paste">Paste &amp; close window</option>
          <option value="none">Disabled (single click fires immediately)</option>
        </select>
      </label>
      <div class="settings-hint">
        When double click is disabled, single click triggers without a 250ms delay.
        The <code>Enter</code> key always pastes and closes (used by keyboard navigation).
        The dedicated copy button on each card always copies regardless of these settings.
      </div>
    </section>
  {/if}

  {#if activeTab === "permissions"}
  <section class="settings-section">
    <div class="settings-section-title">Permissions</div>
    <div class="status-step">
      <div class="status-row">
        <span class="status-dot" class:ok={accessibilityGranted === true} class:fail={accessibilityGranted === false} class:checking={accessibilityGranted === null}></span>
        <span class="status-text">
          {accessibilityGranted === null ? "Checking..." : accessibilityGranted ? "Accessibility granted" : "Accessibility not granted"}
        </span>
        {#if accessibilityGranted === false}
          <button class="status-action" type="button" onclick={async () => { accessibilityGranted = await requestAccessibility(); }}>
            Request
          </button>
        {:else if accessibilityGranted === true}
          <button class="status-action" type="button" onclick={async () => { accessibilityGranted = await checkAccessibility(); }}>
            Recheck
          </button>
        {/if}
      </div>
      {#if accessibilityGranted === false}
        <div class="status-hint">
          Required for paste automation (Cmd+V) and global shortcut.
          Click "Request" to open System Settings, then enable Copyosity under Privacy → Accessibility.
        </div>
      {/if}
    </div>
  </section>

  <section class="settings-section">
    <div class="settings-section-title">Danger zone</div>
    <button class="settings-item danger" type="button" onclick={handleClearHistory}>
      Clear unpinned history
    </button>
  </section>
  {/if}

  {#if activeTab === "ai"}
  <section class="settings-section">
    <div class="settings-section-title">Local AI Status</div>

    {#if ollamaStatus === null}
      <div class="status-row">
        <span class="status-dot checking"></span>
        <span class="status-text">Checking...</span>
      </div>
    {:else}
      <!-- Step 1: Ollama installed -->
      <div class="status-step">
        <div class="status-row">
          <span class="status-dot" class:ok={ollamaStatus.cli_installed} class:fail={!ollamaStatus.cli_installed}></span>
          <span class="status-text">
            {ollamaStatus.cli_installed ? "Ollama installed" : "Ollama not installed"}
          </span>
          {#if !ollamaStatus.cli_installed}
            <button class="status-action" type="button" onclick={() => openUrl("https://ollama.com/download")}>
              Open ollama.com
            </button>
          {/if}
        </div>
        {#if !ollamaStatus.cli_installed}
          <div class="status-hint">
            Ollama runs AI models locally on your machine. Download it from
            <button class="link-btn" type="button" onclick={() => openUrl("https://ollama.com/download")}>ollama.com</button>,
            install the app, and click "Check again".
          </div>
        {/if}
      </div>

      <!-- Step 2: Server running -->
      <div class="status-step">
        <div class="status-row">
          <span class="status-dot" class:ok={ollamaStatus.server_running} class:fail={ollamaStatus.cli_installed && !ollamaStatus.server_running} class:disabled={!ollamaStatus.cli_installed}></span>
          <span class="status-text" class:dimmed={!ollamaStatus.cli_installed}>
            {ollamaStatus.server_running ? "Server running" : "Server not running"}
          </span>
          {#if ollamaStatus.cli_installed && !ollamaStatus.server_running}
            <button class="status-action" type="button" disabled={ollamaLoading} onclick={handleStartServer}>
              {#if ollamaLoading}<span class="spinner"></span> Starting...{:else}Start{/if}
            </button>
          {/if}
        </div>
        {#if ollamaStatus.cli_installed && !ollamaStatus.server_running}
          <div class="status-hint">
            Ollama server is not running. Click "Start" to launch it, or run
            <button class="cmd-code" type="button" onclick={() => copyCmd('ollama serve')} title="Click to copy">{copiedCmd === 'ollama serve' ? 'copied!' : 'ollama serve'}</button> in your terminal.
          </div>
        {/if}
      </div>

      <!-- Step 3: Model installed -->
      <div class="status-step">
        <div class="status-row">
          <span class="status-dot" class:ok={ollamaStatus.model_installed} class:fail={ollamaStatus.server_running && !ollamaStatus.model_installed} class:disabled={!ollamaStatus.server_running}></span>
          <span class="status-text" class:dimmed={!ollamaStatus.server_running}>
            {ollamaStatus.model_installed ? `Model ready` : `Model not installed`}
          </span>
          {#if ollamaStatus.server_running && !ollamaStatus.model_installed}
            <button class="status-action" type="button" disabled={ollamaLoading} onclick={handlePullModel}>
              {#if ollamaLoading}<span class="spinner"></span> Pulling...{:else}Download{/if}
            </button>
          {/if}
          {#if ollamaStatus.model_installed}
            <button class="status-action" type="button" onclick={async () => { await unloadOllamaModel(); settingsNotice = "Model unloaded from memory"; }}>
              Unload
            </button>
          {/if}
        </div>
        {#if pullProgress}
          <div class="status-hint pull-progress">
            <span class="spinner"></span> {pullProgress}
          </div>
        {:else if ollamaStatus.server_running && !ollamaStatus.model_installed}
          {@const pullCmd = `ollama pull ${ollamaStatus.model_name}`}
          <div class="status-hint">
            Model
            <button class="cmd-code" type="button" onclick={() => copyCmd(ollamaStatus!.model_name)} title="Click to copy">{copiedCmd === ollamaStatus.model_name ? 'copied!' : ollamaStatus.model_name}</button>
            needs to be downloaded.
            Click "Download" or run
            <button class="cmd-code" type="button" onclick={() => copyCmd(pullCmd)} title="Click to copy">{copiedCmd === pullCmd ? 'copied!' : pullCmd}</button>
            in terminal.
            This may take a few minutes depending on your connection.
          </div>
        {:else if ollamaStatus.model_installed}
          <div class="status-hint ok">
            Using
            <button class="cmd-code" type="button" onclick={() => copyCmd(ollamaStatus!.model_name)} title="Click to copy">{copiedCmd === ollamaStatus.model_name ? 'copied!' : ollamaStatus.model_name}</button>
          </div>
        {/if}
      </div>

      <!-- Step 4: Tagging test -->
      <div class="status-step">
        <div class="status-row">
          <span class="status-dot" class:ok={taggingResult !== undefined && taggingResult !== null} class:fail={taggingResult === null} class:disabled={!ollamaStatus.model_installed}></span>
          <span class="status-text" class:dimmed={!ollamaStatus.model_installed}>
            {#if taggingResult === undefined}
              Tagging not tested
            {:else if taggingResult !== null}
              Tagging works
            {:else}
              Tagging failed
            {/if}
          </span>
          {#if ollamaStatus.model_installed}
            <button class="status-action" type="button" disabled={taggingLoading || modelDirty} onclick={handleTestTagging} title={modelDirty ? "Save settings first" : ""}>
              {#if taggingLoading}
                <span class="spinner"></span> Testing...
              {:else}
                Test
              {/if}
            </button>
          {/if}
        </div>
        {#if modelDirty}
          <div class="status-hint fail">
            Model changed — save settings first, then test.
          </div>
        {:else if taggingLoading}
          <div class="status-hint">
            Sending test request... This can take up to 60 seconds on first run while the model loads into memory.
          </div>
        {:else if taggingResult !== undefined && taggingResult !== null}
          <div class="status-hint ok">
            Test result: {taggingResult.join(", ")}
          </div>
        {:else if taggingResult === null}
          <div class="status-hint fail">
            The model did not return tags. Try a different model or check Ollama logs.
          </div>
        {:else if ollamaStatus.model_installed}
          <div class="status-hint">
            Click "Test" to verify that the model can tag clipboard content.
          </div>
        {/if}
      </div>

      <button class="settings-ghost-btn refresh-btn" type="button" disabled={ollamaLoading} onclick={refreshOllamaStatus}>
        Check again
      </button>
    {/if}
  </section>

  <section class="settings-section">
    <div class="settings-section-title">AI Model</div>
    <label class="settings-field">
      <span class="settings-label">Ollama model</span>
      <select
        class="settings-select"
        bind:value={selectedModelPreset}
        onchange={(e) => handleModelPresetChange((e.currentTarget as HTMLSelectElement).value)}
      >
        {#each modelCatalog.options as option}
          <option value={option.value}>
            {option.label} · ~{option.memory_gb.toFixed(1)} GB · {option.fits ? "fits" : "tight"}{option.installed ? " · installed" : ""}
          </option>
        {/each}
        <option value="__custom__">Custom model</option>
      </select>
      {#if selectedModelPreset === "__custom__"}
        <input
          class="settings-input"
          type="text"
          bind:value={settings.ollama_model}
          placeholder="qwen3:4b-instruct-2507-q4_K_M"
        />
      {/if}
      <div class="settings-info-card">
        <div class="settings-hint">
          Machine RAM: {modelCatalog.total_memory_gb.toFixed(1)} GB
        </div>
        <div class="settings-hint">
          Recommended Ollama budget: {modelCatalog.recommended_memory_gb.toFixed(1)} GB
        </div>
        {#if selectedModelMeta}
          <div class="settings-hint" class:fits={selectedModelMeta.fits} class:tight={!selectedModelMeta.fits}>
            {selectedModelMeta.label} needs about {selectedModelMeta.memory_gb.toFixed(1)} GB and
            {selectedModelMeta.fits ? " should fit this machine." : " may be too heavy for this machine."}
          </div>
        {/if}
      </div>
    </label>
  </section>
  {/if}

  {#if activeTab === "privacy"}
  <section class="settings-section">
    <div class="settings-section-title">Privacy</div>
    <div class="settings-field">
      <span class="settings-label">Excluded apps</span>
      <div class="settings-inline">
        <input
          class="settings-input"
          type="text"
          bind:value={excludedAppInput}
          placeholder="App name, for example Telegram"
        />
        <button class="settings-small-btn" type="button" onclick={handleAddExcludedApp}>
          Add
        </button>
      </div>
      <button class="settings-ghost-btn" type="button" onclick={handleAddFrontmostApp}>
        Exclude current app
      </button>
      {#if excludedApps.length > 0}
        <div class="excluded-apps">
          {#each excludedApps as app}
            <div class="excluded-app-row">
              <span class="excluded-app-name">{app.bundle_id}</span>
              <button
                class="excluded-remove-btn"
                type="button"
                onclick={() => handleRemoveExcludedApp(app.id)}
              >
                Remove
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <div class="settings-hint">Clipboard from excluded apps will not be stored or tagged.</div>
      {/if}
    </div>
  </section>
  {/if}

  <div class="settings-actions">
    <button class="settings-save-btn" type="button" disabled={savingSettings} onclick={saveSettings}>
      {savingSettings ? "Saving..." : "Save settings"}
    </button>
    {#if settingsNotice}
      <div class="settings-note">{settingsNotice}</div>
    {/if}
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    background: rgba(30, 30, 36, 0.96);
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
    font-size: var(--text-md);
    color: var(--fg-primary);
    user-select: none;
    -webkit-user-select: none;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
    overflow-x: hidden;
    overscroll-behavior: none;
  }

  :global(*) {
    box-sizing: border-box;
  }

  .settings-page {
    padding: 44px var(--space-5) var(--space-5);
    max-width: 520px;
    margin: 0 auto;
  }

  /* Restart banner */
  .restart-banner {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
    padding: var(--space-3) var(--space-3);
    background: var(--warning-bg);
    border: 1px solid var(--warning-border);
    border-radius: var(--radius-lg);
  }

  .restart-banner-text {
    flex: 1;
    font-size: var(--text-sm);
    color: var(--warning);
    line-height: var(--leading-body);
  }

  .restart-banner-btn {
    min-height: var(--control-sm);
    padding: 0 var(--space-3);
    background: rgba(217, 165, 90, 0.18);
    border: 1px solid var(--warning-border);
    border-radius: var(--radius-md);
    color: var(--warning);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.15s ease, transform 0.08s ease;
  }

  .restart-banner-btn:hover { background: rgba(217, 165, 90, 0.28); }

  /* Tabs */
  .settings-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    margin-bottom: var(--space-4);
    padding: 3px;
    background: var(--surface-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
  }

  .settings-tab {
    flex: 1 1 auto;
    min-width: 70px;
    padding: 6px var(--space-3);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--fg-secondary);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease, transform 0.08s ease;
  }

  .settings-tab:hover { color: var(--fg-primary); background: var(--surface-2); }
  .settings-tab:active { transform: scale(0.98); background: var(--surface-3); }
  .settings-tab.active {
    background: var(--accent-bg-strong);
    color: var(--fg-primary);
  }

  /* Sections */
  .settings-section {
    padding: var(--space-4);
    background: var(--surface-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-xl);
  }

  .settings-section + .settings-section { margin-top: var(--space-3); }

  .settings-section-title {
    margin-bottom: var(--space-3);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-muted);
  }

  /* Fields */
  .settings-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .settings-field + .settings-field { margin-top: var(--space-3); }

  .settings-inline { display: flex; gap: var(--space-2); align-items: center; }

  .settings-label {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--fg-secondary);
  }

  .settings-input,
  .settings-select {
    width: 100%;
    min-height: var(--control-md);
    padding: 6px var(--space-3);
    background: var(--surface-2);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-md);
    color: var(--fg-primary);
    font: inherit;
    font-size: var(--text-md);
    outline: none;
    transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease;
  }

  .settings-select {
    appearance: none;
    -webkit-appearance: none;
    -moz-appearance: none;
    padding-right: 32px;
    background-image:
      linear-gradient(45deg, transparent 50%, var(--fg-secondary) 50%),
      linear-gradient(135deg, var(--fg-secondary) 50%, transparent 50%);
    background-position: calc(100% - 14px) calc(50% - 2px), calc(100% - 10px) calc(50% - 2px);
    background-size: 5px 5px;
    background-repeat: no-repeat;
    cursor: pointer;
  }

  .settings-select:hover { background-color: var(--surface-3); border-color: var(--border-strong); }
  .settings-select option { color: var(--fg-primary); background: #23252c; }

  .settings-input:focus,
  .settings-select:focus {
    border-color: var(--accent-border);
    box-shadow: 0 0 0 3px var(--accent-bg);
  }

  .settings-input::placeholder { color: var(--fg-disabled); }

  .settings-info-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-3);
    background: var(--surface-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }

  .settings-hint {
    font-size: var(--text-xs);
    line-height: var(--leading-body);
    color: var(--fg-muted);
  }

  .settings-hint code {
    display: inline-block;
    padding: 0 5px;
    background: var(--surface-2);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    font-family: "SF Mono", Menlo, monospace;
    font-size: 0.92em;
    line-height: 1.5;
    color: var(--fg-primary);
    vertical-align: baseline;
  }

  .cmd-code {
    display: inline-block;
    padding: 1px 7px;
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    border-radius: 4px;
    font-family: "SF Mono", Menlo, monospace;
    font-size: 0.92em;
    line-height: 1.5;
    color: var(--fg-primary);
    cursor: pointer;
    vertical-align: baseline;
    transition: background 0.15s ease, color 0.15s ease, transform 0.08s ease;
  }
  .cmd-code:hover { background: var(--accent-bg-strong); }
  .cmd-code:active { transform: scale(0.96); }

  .settings-hint.fits { color: var(--success); }
  .settings-hint.tight { color: var(--warning); }

  /* Toggle */
  .settings-toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
    cursor: pointer;
    user-select: none;
  }

  .settings-toggle input[type="checkbox"] {
    width: 14px;
    height: 14px;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .settings-toggle-label {
    font-size: var(--text-md);
    font-weight: 500;
    color: var(--fg-primary);
  }

  /* Buttons (secondary / ghost) */
  .settings-small-btn,
  .settings-ghost-btn {
    min-height: var(--control-md);
    border-radius: var(--radius-md);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, transform 0.08s ease;
  }

  .settings-small-btn {
    padding: 0 var(--space-3);
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    color: var(--fg-primary);
    white-space: nowrap;
  }

  .settings-small-btn:hover { background: var(--accent-bg-strong); }
  .settings-small-btn:active { transform: scale(0.98); background: rgba(107, 141, 214, 0.30); }

  .settings-ghost-btn {
    padding: 0 var(--space-3);
    background: var(--surface-2);
    border: 1px solid var(--border-default);
    color: var(--fg-secondary);
    width: fit-content;
  }

  .settings-ghost-btn:hover { background: var(--surface-3); color: var(--fg-primary); }
  .settings-ghost-btn:active { transform: scale(0.98); background: rgba(255, 255, 255, 0.12); }

  /* Save button — primary */
  .settings-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-3);
    margin-top: var(--space-5);
  }

  .settings-save-btn {
    min-height: var(--control-md);
    padding: 0 var(--space-4);
    background: var(--accent);
    border: 1px solid var(--accent);
    border-radius: var(--radius-md);
    color: #ffffff;
    font: inherit;
    font-size: var(--text-sm);
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s ease, opacity 0.15s ease, transform 0.08s ease;
  }

  .settings-save-btn:hover:not(:disabled) { background: var(--accent-hover); }
  .settings-save-btn:active:not(:disabled) { transform: scale(0.98); background: #5a7cc3; }
  .settings-save-btn:disabled { opacity: 0.5; cursor: default; }

  .settings-note {
    font-size: var(--text-xs);
    color: var(--success);
  }

  /* Excluded apps list */
  .excluded-apps {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    max-height: 160px;
    overflow-y: auto;
  }

  .excluded-app-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }

  .excluded-app-name {
    font-size: var(--text-sm);
    color: var(--fg-primary);
    min-width: 0;
    word-break: break-word;
  }

  .excluded-remove-btn {
    border: none;
    background: transparent;
    color: var(--warning);
    cursor: pointer;
    font: inherit;
    font-size: var(--text-xs);
    padding: 0;
    white-space: nowrap;
  }

  .excluded-remove-btn:hover { color: #e8b76d; }

  /* Danger item */
  .settings-item {
    width: 100%;
    min-height: var(--control-md);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--fg-primary);
    text-align: left;
    cursor: pointer;
    font: inherit;
    font-size: var(--text-sm);
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease, transform 0.08s ease;
  }

  .settings-item.danger { color: var(--danger); }
  .settings-item.danger:hover { background: var(--danger-bg); border-color: rgba(215, 122, 122, 0.22); }

  /* Status rows */
  .status-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
  }

  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--surface-3);
  }
  .status-dot.ok       { background: var(--success); box-shadow: 0 0 5px rgba(110, 207, 138, 0.5); }
  .status-dot.fail     { background: var(--danger); box-shadow: 0 0 5px rgba(215, 122, 122, 0.5); }
  .status-dot.checking { background: var(--warning); animation: pulse 1s infinite; }
  .status-dot.disabled { background: var(--surface-3); }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  .status-text {
    flex: 1;
    font-size: var(--text-sm);
    color: var(--fg-primary);
  }
  .status-text.dimmed { color: var(--fg-muted); }

  .status-action {
    padding: 0 var(--space-3);
    min-height: var(--control-sm);
    border-radius: var(--radius-sm);
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    color: var(--fg-primary);
    font: inherit;
    font-size: var(--text-xs);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.15s ease, transform 0.08s ease;
  }
  .status-action:hover:not(:disabled) { background: var(--accent-bg-strong); }
  .status-action:active:not(:disabled) { transform: scale(0.97); background: rgba(107, 141, 214, 0.30); }
  .status-action:disabled { opacity: 0.5; cursor: default; }

  .refresh-btn {
    margin-top: var(--space-2);
    min-height: var(--control-sm);
    font-size: var(--text-xs);
  }

  .status-step { padding: var(--space-2) 0; }
  .status-step + .status-step { border-top: 1px solid var(--border-subtle); }

  .status-hint {
    margin: var(--space-1) 0 0 var(--space-4);
    font-size: var(--text-xs);
    line-height: var(--leading-body);
    color: var(--fg-muted);
  }

  .status-hint.ok { color: var(--success); }
  .status-hint.fail { color: var(--danger); }

  .link-btn {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
    font: inherit;
    font-size: var(--text-xs);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .link-btn:hover { color: var(--accent-hover); }

  .spinner {
    display: inline-block;
    width: 9px;
    height: 9px;
    border: 1.5px solid rgba(255, 255, 255, 0.2);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
    vertical-align: middle;
  }

  @keyframes spin { to { transform: rotate(360deg); } }

  .update-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .update-status {
    flex: 1;
    font-size: var(--text-sm);
    color: var(--fg-primary);
    line-height: var(--leading-tight);
  }

  .update-status.update-muted { color: var(--fg-muted); }
  .update-status.update-error { color: var(--danger); }

  .update-check-btn { margin-left: auto; }

  .pull-progress {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--accent);
    font-family: "SF Mono", Menlo, monospace;
    font-size: 10.5px;
    word-break: break-all;
  }
</style>
