# zellij-quill

[![Build WASM](https://github.com/krcz/zellij-quill/actions/workflows/build-wasm.yml/badge.svg)](https://github.com/krcz/zellij-quill/actions/workflows/build-wasm.yml)

`zellij-quill` is a vibe-coded Zellij plugin that exposes pane automation primitives over `zellij pipe`.
It is designed for scripting and agent workflows: send input to panes, read/grep/wait on scrollback, run commands, and manage token-based access.

**Note: I haven't reviewed the code yet; I should do it in the next few days. Consider the software pre-alpha and use at own risk.**

## Features

- Pane discovery: `panes`
- Pane input: `send`, `run`, `interrupt`
- Scrollback operations: `tail`/`read`, `grep`, `wait`
- Host commands: `exec`
- Pane creation: `spawn`
- Scrollback marks: `mark`, `since`
- Auth and access control: `token`, `permit` (status only; approvals are UI-only)

## Requirements

- **Disclaimer:** this project currently requires a development version of Zellij built from GitHub. Stable release builds are not supported.
- Rust toolchain (stable)
- `wasm32-wasip1` target installed
- Zellij with plugin APIs used by this project (including pane scrollback access)

## Build

```bash
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
```

WASM artifact:

```text
target/wasm32-wasip1/release/zellij_quill.wasm
```

Optional verification:

```bash
cargo fmt
cargo check
cargo test
cargo check --target wasm32-wasip1
```

## Enable In Zellij Config

Add the plugin to your Zellij config (usually `~/.config/zellij/config.kdl`) so it is loaded in the background when a session starts:

```kdl
load_plugins {
  "https://github.com/krcz/zellij-quill/releases/download/v0.0.2/zellij_quill.wasm" {
    require_token false
    enable_pane_permissions true
    // token "your-static-token" // optional
  }
}
```

You can use `file:/home/user/zellij-quill/target/wasm32-wasip1/release/zellij_quill.wasm` if you prefer local build.

Then restart Zellij (or start a new session) so the config is reloaded.

## Usage

Load the compiled plugin in Zellij using your normal plugin workflow, then send commands via pipe.
Examples below assume the pipe name is `quill`.

### Basic commands

```bash
zellij pipe --name quill -- 'panes --json'
zellij pipe --name quill -- 'send --pane editor -- "echo hello"'
zellij pipe --name quill -- 'run --pane editor -- "cargo test -q"'
zellij pipe --name quill -- 'tail --pane editor --lines 100'
zellij pipe --name quill -- 'grep --pane editor -i --last 2000 "error"'
zellij pipe --name quill -- 'wait --pane editor --regex "DONE" --timeout 30s'
zellij pipe --name quill -- "exec -- sh -lc 'echo hi; echo err >&2; exit 7'"
zellij pipe --name quill -- 'spawn --kind terminal --where tiled'
```

All pane-targeting commands require `--pane <name>` with an exact terminal pane title.
Selectors like `focused`, `id:3`, and regex selectors are not supported.

### Output format

- Default mode: human-readable output plus a final `@@json ...` line.
- `--json`: machine-readable JSON only.

### Auth and permissions

Config keys:

- `require_token` (default: `false`)
- `token` (optional configured auth token)
- `enable_pane_permissions` (default: `true`)

Token command:

```bash
zellij pipe --name quill -- 'token'
zellij pipe --name quill -- 'token --json'
```

If `ZELLIJ_QUILL_TOKEN` exists, it is reused. Otherwise a token is generated and returned.
Because plugins cannot set session env vars directly, JSON output includes an `export_command` you can run manually.

Pane permissions map tokens to allowed panes. If a command tries to access/create a pane without permission, quill:

- writes a permission prompt message into the request-origin pane
- opens a `quill-approval` command pane with an interactive `y/N` prompt
- returns `PERMISSION_REQUIRED` with a `request_id`
- blocks the active `zellij pipe` request and retries automatically after approval

To route permission prompts to the calling pane, include `ZELLIJ_PANE_ID` in pipe args:

```bash
zellij pipe --name quill --args ZELLIJ_PANE_ID=$ZELLIJ_PANE_ID -- 'send --pane editor -- "echo hi"'
```

Approve request:

```bash
In the `quill-approval` pane: type `y` + Enter to approve, or anything else + Enter to deny.
```

Inspect pending requests (read-only):

```bash
zellij pipe --name quill -- 'permit --json'
```

### Command list

`panes`, `send`, `run`, `tail`, `read`, `grep`, `wait`, `exec`, `spawn`, `interrupt`, `mark`, `since`, `token`, `permit`, `help`
