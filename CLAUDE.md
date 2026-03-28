# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**danmuji-next** — Bilibili live stream danmaku (barrage) desktop tool. Tauri 2 (Rust backend) + Vue 3 + TypeScript + Pinia. Multi-window architecture where each tab (danmaku, gifts, superchat, audience) can be a separate always-on-top window.

## Commands

```bash
pnpm install          # Install frontend dependencies
pnpm td               # Dev mode (Tauri + Vite HMR, frontend on localhost:1420)
pnpm build1           # Build distributable app

# Rust only
cd src-tauri
cargo check           # Type check
cargo build           # Build backend
cargo test            # Run tests (currently minimal)
```

## Architecture

### Frontend-Backend Communication
- **Tauri Commands** (`invoke()`) for request/response operations
- **Event System** (`emit`/`listen`) for real-time push from Rust to Vue
- Events are namespaced per window: `blive-data:{windowLabel}`, `blive-status`
- Windows subscribe to specific event types; backend filters updates per subscription

### Multi-Window System
- Windows defined in `src-tauri/src/window_state.rs` (`WindowConfig`)
- Each window has a label (e.g., "main", "danmaku", "settings", "archive") and its own Vue Router route
- Window state (position, size, open-state) persisted to KV store (`window_states.json`), separate from user settings
- `src/services/window-manager.ts` handles creation, state persistence, and auto-restore on startup

### Data Flow (Live)
```
Bilibili WebSocket → blivedm/client.rs (packet decode, event parse)
  → blive_service.rs (process, merge gifts, update stats, buffer)
  → 100ms push interval → emit filtered DataUpdate events to subscribed windows
  → also clone to archive mpsc channel → archive writer batches to SQLite
```

### Key Backend Modules
- **`blivedm/`** — Self-contained Bilibili live protocol library (WebSocket client, packet codec, message parsers)
- **`blive_service.rs`** — Orchestrates connection lifecycle, event processing, gift merging (5s window), stats, window subscriptions
- **`archive.rs`** — SQLite persistence for historical sessions; async writer via mpsc channel
- **`crypto.rs`** — Cookie encryption using Windows DPAPI (transparent to frontend)
- **`commands.rs`** — All `#[tauri::command]` functions exposed to frontend

### Key Frontend Modules
- **`stores/danmaku.ts`** — Pinia store for live data (danmaku, gifts, SC, stats, contribution ranks)
- **`stores/settings.ts`** — User settings with auto-save (1s debounce)
- **`services/blive-client.ts`** — Wraps Tauri commands for connecting/subscribing
- **`components/items/`** — Renderers for each data type (DanmakuItem, GiftItem, SuperChatItem)

### Storage
| File | Purpose |
|------|---------|
| `config.json` | User settings (cookie encrypted via DPAPI, room ID, display prefs) |
| `window_states.json` | Window positions/sizes (KV store) |
| `archives.db` | SQLite: recorded sessions, danmaku, gifts, super_chats |

All in `dirs::config_dir()/danmuji-next/` (typically `%APPDATA%/danmuji-next/`).

## Conventions

- Comments and UI text in **Chinese**; code identifiers in English
- No external UI framework — custom dark theme using CSS variables (`--bg-primary`, `--text-primary`, etc.)
- Transparent, borderless windows with custom `TitleBar.vue` component
- Bilibili image URLs require `referrerpolicy="no-referrer"` and `crossorigin="anonymous"` to bypass hotlink protection
- Frontend uses hash-mode Vue Router (`createWebHashHistory`)
- Rust errors returned as `Result<T, String>` from commands
- `DANMUJI_NEXT_DEV` env var enables dev mode (raw event dump to file)
