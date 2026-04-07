# Tauri + Vue + TypeScript

This template should help get you started developing with Vue 3 and TypeScript in Vite. The template uses Vue 3 `<script setup>` SFCs, check out the [script setup docs](https://v3.vuejs.org/api/sfc-script-setup.html#sfc-script-setup) to learn more.

## Release Workflow

- Local bump only: `python scripts/bump_version_and_build.py --no-build`
- Local bump + build: `python scripts/bump_version_and_build.py`
- Bump minor version + build: `python scripts/bump_version_and_build.py minor`
- After committing the version change, push a tag like `v1.6.1` to trigger GitHub Actions release build and publish.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
