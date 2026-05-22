# Copyosity

A fast, privacy-first clipboard manager for macOS. Lives in your menu bar, opens instantly with a hotkey, and never steals focus from your active app.

Built with Tauri 2, Svelte 5, Rust, and SQLite.

> This is a fork of [vakovalskii/copyosity](https://github.com/vakovalskii/copyosity) with a reworked
> Settings UI and configurable behavior.

## Why Copyosity

- **No focus stealing** — uses macOS NSPanel, your cursor stays exactly where it was
- **Local AI tagging** — automatic smart tags via Ollama, everything runs on your machine
- **Instant access** — global hotkey opens history in ~100ms, Escape hides it
- **Privacy by design** — no cloud, no telemetry, clipboard stays in local SQLite

## Features

### Clipboard History
- Automatic capture of text and images from all apps
- Horizontal card-based UI with source app labels
- Search across all clipboard text
- Configurable retention (1 day to 6 months)

### Configurable Click Behavior
- **Single click action** — choose `Copy to clipboard` or `Paste & close window`
- **Double click action** — choose `Paste & close`, `Copy`, or disable (single click fires immediately)
- **Copy button** (⎘) on each card — always copies regardless of click settings
- **"Copied" animation** — visual confirmation before the window collapses
- **Keyboard navigation** — arrow keys to browse, Enter to paste, Escape to dismiss

### AI Tagging
- Automatic tagging powered by local Ollama (Qwen3 models)
- Step-by-step setup in Settings: install check, server status, model download, tagging test
- Filter by tags — quickly find URLs, code snippets, meeting notes, etc.
- Heuristic detection for OTPs, tokens, and opaque codes (no AI needed)

### Organization
- **Starred items** — pin important clips to keep them forever
- **Collections** — group clips into custom tabs
- **Excluded apps** — block specific apps from being recorded (passwords, banking, etc.)

### Settings (tabbed)
- **General** — configurable main hotkey, Show in Dock toggle, history retention
- **Behavior** — single/double click actions
- **AI & Tags** — Ollama wizard and model selection
- **Voice** — Whisper transcription endpoint + hold-to-record hotkey
- **Privacy** — excluded apps
- **Permissions** — Accessibility check, clear history

### System Integration
- Menu bar tray icon (pink + turquoise)
- Default global shortcut: `Cmd + Shift + V` (configurable in Settings → General)
- Optionally runs as macOS Accessory (no Dock icon) — toggle in Settings → General
- macOS code-signed and notarized (signing is configured for the upstream author —
  unsigned local builds work for personal use, see Development below)

## Install

This fork doesn't ship binaries yet. Build from source:

```bash
git clone <this-fork-url>
cd copyosity
npm install
npm run tauri build
# result: src-tauri/target/release/bundle/macos/Copyosity.app
#         src-tauri/target/release/bundle/dmg/Copyosity_0.3.1_aarch64.dmg
```

Then drag `Copyosity.app` into `/Applications`. On first launch macOS may complain that
the app is unsigned by an identified developer. Open it via **System Settings →
Privacy & Security → Open anyway**, or run:

```bash
xattr -dr com.apple.quarantine /Applications/Copyosity.app
```

### Permissions

macOS will ask for:
- **Accessibility** — needed for paste automation (Cmd+V simulation) and global shortcut
- **Input Monitoring** — may be required for reliable hotkey detection

### Local AI (optional)

For automatic clipboard tagging:
1. Install [Ollama](https://ollama.com/download)
2. Open Copyosity Settings → AI & Tags — follow the step-by-step status panel
3. The app will start the server and download the model for you

## Usage

| Action | What it does |
|--------|-------------|
| Main hotkey (default `Cmd + Shift + V`) | Open / close clipboard history |
| Voice hotkey (default `Option + Space`, hold) | Record → transcribe → paste at cursor |
| Single click on card | Configurable in Settings → Behavior |
| Double click on card | Configurable in Settings → Behavior |
| `Escape` | Hide window |
| Arrow keys + `Enter` | Navigate and paste |
| Click ⎘ button | Copy without closing |
| Click ★ button | Star / unstar |
| Click gear icon | Open Settings |

## Privacy

- All data stored locally in `~/Library/Application Support/com.vkovalskii.copyosity/`
- AI tagging runs on `127.0.0.1` via Ollama — nothing leaves your machine
- Voice transcription uses the Whisper-compatible URL you configure in Settings → Voice
  (defaults to empty — feature off until you point it somewhere)
- Exclude sensitive apps in Settings → Privacy
- Clear history anytime from Settings → Permissions

## Development

```bash
npm install
npm run tauri dev
```

### Checks

```bash
npm run check              # Svelte + TypeScript
cd src-tauri && cargo test # 39 unit tests
cd src-tauri && cargo check
```

### Release

```bash
make release-macos    # Build, sign, notarize
make notarize-info    # Check notarization status
```
