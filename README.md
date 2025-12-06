# Clippa

Desktop YouTube clipper built with Tauri + React. It downloads trimmed segments with `yt-dlp`, saves them to a local library, and lets you replay, open, or delete clips from inside the app.

## Prerequisites
- Node.js 18+ and pnpm (`corepack enable` or `npm i -g pnpm`)
- Rust toolchain (`rustup`), needed by Tauri
- System tools: `yt-dlp` and `ffmpeg` available on your `$PATH` (macOS/Homebrew: `brew install yt-dlp ffmpeg`)

## Install
```bash
pnpm install
```
This pulls front-end deps and the Tauri CLI.

## Run the app (development)
```bash
pnpm tauri dev
```
This starts Vite and the Tauri shell, opening the desktop window with hot reload.

## Build a release bundle
```bash
pnpm tauri build
```
The native binaries/installer are emitted under `src-tauri/target` (git-ignored).

## Data & clips
- Clips and a small SQLite DB are stored under your OS local data directory in `Clippa/` (for example `~/Library/Application Support/Clippa` on macOS, `~/.local/share/Clippa` on Linux, `%LOCALAPPDATA%\\Clippa` on Windows).
- Ensure `yt-dlp` and `ffmpeg` stay installed; the app shells out to them for downloads and merging.
