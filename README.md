# Kilo App

A fast, native **Windows desktop client for the [Kilo Gateway](https://kilo.ai)** — an
OpenAI-compatible LLM API. Chat with KiloCode Free models, and let Kilo work directly in
your project folder: reading files, making focused edits and running commands.

Built with [Tauri 2](https://tauri.app) + Vue 3 + TypeScript. The UI runs in the system
WebView2 rather than a bundled browser, so the whole app is ~17 MB and starts instantly.

**[Website](https://ahpah-dev.github.io/Kilo-App/)** · **[Download](../../releases/latest)** · **[Report an issue](../../issues)**

---

## Features

- **Agentic coding in your workspace** — open a folder and Kilo explores it with real
  tools instead of guessing at file contents.
- **Seven built-in tools** — `list_directory`, `read_file`, `search_files`, `write_file`,
  `edit_file`, `delete_path` and `run_command`.
- **Approval before anything destructive** — reads run freely; writes and commands ask
  first. Auto-approve can be enabled per category in Settings.
- **Streaming responses** — tokens, visible reasoning and live tool-call cards as they
  happen.
- **Diff review** — every edit is rendered as a colour-coded diff, with a list of changed
  files per conversation.
- **KiloCode Free models** — models tagged `:free` are discovered live from the gateway
  and clearly marked. No account is needed to start chatting.
- **Optional API key** — add a Kilo API key for higher limits. It is validated against the
  gateway and stored locally.
- **13 themes and 3 interface modes** — pick any colour theme, then render it **Polished**
  (layered surfaces and soft depth), **Glass** (translucent panels with blurred depth) or
  **Clean** (flat, hairline borders, minimal). All with matching syntax highlighting.
- **Local history** — conversations are stored on your machine and survive restarts.

## Install

1. Download the installer from the [latest release](../../releases/latest).
   It installs per-user, so **no administrator rights are required**.
2. Launch Kilo — it connects to the Kilo Gateway on the free tier straight away.
3. Click **Open folder** to choose a workspace, then describe the change you want.

Windows 10/11 x64 with the WebView2 runtime (preinstalled on Windows 11; the installer
handles it on Windows 10).

## Build from source

**Prerequisites:** Node 18+, Rust 1.90+, and the WebView2 runtime.

```bash
git clone https://github.com/ahpah-dev/Kilo-App.git
cd kilo-app
npm install
npm run tauri dev      # development, with hot reload
npm run tauri build    # production installer
```

The installer is written to `src-tauri/target/release/bundle/nsis/`.

### Checks

```bash
npm run build                       # vue-tsc typecheck + production build
cd src-tauri && cargo test          # Rust unit tests
cd src-tauri && cargo test -- --ignored   # live tests against the real gateway
cd src-tauri && cargo clippy --all-targets
```

## Project layout

```
src/                    Vue 3 frontend
  components/           UI building blocks (chat bubbles, tool cards, title bar)
  stores/               Pinia stores — chat/agent loop, gateway settings
  services/             Tauri IPC bridge and the agent tool definitions
  styles/               Design tokens and global CSS
src-tauri/              Rust backend
  src/gateway.rs        Kilo Gateway client (streaming SSE, API key validation)
  src/tools.rs          Workspace-confined tool implementations
  src/commands.rs       Tauri commands exposed to the frontend
  icons/                Generated app icons (see tools/make-logo.py)
docs/                   The marketing site, published to GitHub Pages
tools/                  Asset generation scripts
```

## Brand assets

The logo is an isometric cube in the app's indigo accent. It is defined once and
generated everywhere from a single script:

```bash
python tools/make-logo.py
```

That writes the full Windows icon set (`src-tauri/icons/`), the web favicon and
logo (`public/`, `docs/`) and the PNG masters in `assets/`. Replacing the icons
requires a **clean** rebuild of the app, because Tauri caches the compiled
Windows resources:

```bash
rm -rf src-tauri/target/release/build/kilo-app-*   # force resource recompile
npm run tauri build
```

## How the agent works

Each turn, the frontend sends the conversation plus the tool definitions to
`POST {gateway}/chat/completions` with `stream: true`. The Rust backend forwards every SSE
chunk to the UI, which accumulates content, reasoning and tool-call argument fragments.
When the model requests tools, each call is checked against the approval policy, executed
inside the workspace, and appended to the conversation as a `tool` message with a matching
`tool_call_id` before the loop continues — up to the configured step limit.

Every filesystem path is resolved against the workspace root, following symlinks on the
longest existing ancestor, so a tool cannot read or write outside the folder you opened.

## Security notes

- Your API key and settings live in the app config directory on your machine and are only
  ever sent to the Kilo Gateway.
- Tool execution is confined to the workspace folder.
- Rendered model output is sanitized with DOMPurify before it reaches the DOM.
- Links in messages open in your system browser, never inside the app.

## Disclaimer

Kilo App is an independent, community-built client for the Kilo Gateway. It is not
affiliated with or endorsed by Kilo. Model access is provided by the gateway and is subject
to its own terms.

## License

[Unlicense](LICENSE) — released into the public domain.
