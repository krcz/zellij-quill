---
name: zellij-quill-control
description: Use this skill to run, supervise, and interact with long-running commands in Zellij panes through the zellij-quill plugin. Trigger this skill when users ask to keep processes running in a pane, drive REPL-like workflows, stream output, wait for readiness signals, send follow-up input, or interrupt/recover interactive jobs via `zellij pipe` or `zellij action pipe`.
---

# Zellij Quill Interactive Pane Control

## Overview

Use `zellij-quill` as a pane automation control plane for long-lived and interactive terminal tasks. Prefer an observe-control loop: launch, monitor, interact, and recover.

## Sandbox Requirement

Run `zellij pipe` and `zellij action pipe` commands outside the sandbox (with escalated permissions). Sandbox-isolated shells often cannot see the user's active Zellij session and may return `There is no active session!`.

Use the exact command prefix `zellij pipe --name quill --` for quill calls. When requesting escalation, prefer the scoped persistent rule `["zellij","pipe","--name","quill","--"]` so this prefix no longer prompts while broader `zellij pipe` usage still requires confirmation.

Keep quill calls simple so approval rules match reliably. Prefer one direct quill command per tool call, and avoid wrapping quill commands inside shell pipelines, command substitution, variable assignment, or command chains (for example `|`, `$()`, `&&`, `;`) unless absolutely necessary.

## Quick Start

1. Confirm plugin availability.
2. Resolve the exact target pane title.
3. Get a quill token and pass it via `--token` (or set `ZELLIJ_QUILL_TOKEN`).
4. Start the long-running command in that pane.
5. Monitor output with `tail`, `grep`, or `wait`.
6. Continue interaction with `send`.
7. Interrupt or recover with `interrupt` and follow-up `send`/`run`.

## Establish Session and Routing

Use one command style consistently:

```bash
# Current attached session
zellij pipe --name quill -- 'panes --json'

# Explicit session targeting
zellij --session <session-name> action pipe --name quill -- 'panes --json'
```

Use quoted payloads so plugin flags are parsed by quill, not by `zellij`.

## Token Authentication

When pane permissions are enabled, pane-access commands require token auth. Without a token, commands fail with `UNAUTHORIZED` and a hint to pass `--token` (or set `ZELLIJ_QUILL_TOKEN`).

Generate or reuse a token:

```bash
zellij pipe --name quill -- 'token --json'
```

Extract the token from the JSON in your caller, then pass it via `--token` on subsequent commands.

Then use `--token` on pane commands:

```bash
zellij pipe --name quill -- "send --pane backend --token $TOKEN -- \"echo warmup\""
```

## Resolve Pane Titles

List panes and use the exact terminal pane title for `--pane`:

```bash
zellij pipe --name quill -- 'panes --json'
```

Target by title only:

```bash
zellij pipe --name quill -- "send --pane backend --token $TOKEN -- \"echo warmup\""
```

## Run Long-Running Commands

Start a job and return immediately:

```bash
zellij pipe --name quill -- "run --pane backend --token $TOKEN -- \"npm run dev\""
```

Start and block until a readiness pattern appears:

```bash
zellij pipe --name quill -- "run --pane backend --token $TOKEN --wait \"ready on\" --timeout 60s -- \"npm run dev\""
```

Use `wait` for explicit polling-style synchronization:

```bash
zellij pipe --name quill -- "wait --pane backend --token $TOKEN --regex \"Migration complete\" --timeout 90s"
```

Prefer quill-native synchronization (`run --wait` or `wait --regex`) over local shell sleeps when coordinating pane state.

## Interact With Running Processes

Send normal input:

```bash
zellij pipe --name quill -- "send --pane backend --token $TOKEN -- \"status\""
```

Send control keys or force interrupt semantics:

```bash
zellij pipe --name quill -- "send --pane backend --token $TOKEN --keys \"<C-c>\""
zellij pipe --name quill -- "interrupt --pane backend --token $TOKEN"
```

Use repeated `send` calls to drive REPL/menu workflows step by step.

`send` appends a newline by default when text is provided, so it usually submits the command immediately. Do not send an extra `<Enter>` unless you intentionally want a second submit. Use `--no-newline` to type without submitting.

For REPL interactions where you need to isolate one evaluation window, use a mark-first sequence:

```bash
zellij pipe --name quill -- "mark --pane julia --token $TOKEN --json"
zellij pipe --name quill -- "send --pane julia --token $TOKEN -- \"2 + 2\""
zellij pipe --name quill -- "tail --pane julia --token $TOKEN --since <mark-token> --lines 80"
```

## Monitor and Inspect Output

Tail recent output:

```bash
zellij pipe --name quill -- "tail --pane backend --token $TOKEN --lines 200"
```

Search recent output for failures/signals:

```bash
zellij pipe --name quill -- "grep --pane backend --token $TOKEN -i --last 4000 \"error|panic|failed\""
```

Use marks to track incremental progress windows:

```bash
zellij pipe --name quill -- "mark --pane backend --token $TOKEN --json"
zellij pipe --name quill -- "tail --pane backend --token $TOKEN --since <mark-token> --lines 200"
```

For commands that intentionally print no output (for example `sleep(...)` in a REPL), treat completion as the prompt reappearing via `run --wait "<prompt-regex>"` or `wait --regex "<prompt-regex>"`.

## Permission-Gated Flows

If pane permissions are enabled, include the origin pane id so approval UX is shown in the requester pane:

```bash
zellij pipe --name quill --args ZELLIJ_PANE_ID=$ZELLIJ_PANE_ID -- "run --pane backend --token $TOKEN -- \"npm run dev\""
```

If a command returns `PERMISSION_REQUIRED`, approve in the `quill-approval` prompt pane (`y` + Enter). Then rerun the same command.

## Reliable Control Pattern

Use this loop for robust long-running workflows:

1. Start: `run --pane ... -- "<command>"`
2. Wait for readiness: `wait --pane ... --regex ... --timeout ...`
3. Interact: `send --pane ... -- "<input>"`
4. Observe: `tail`/`grep`
5. Recover on failure: `interrupt`, then restart with `run`

## Troubleshooting

If pipe output is empty:

1. Verify a quill instance is actually loaded.
2. Use explicit session targeting: `zellij --session <name> action pipe ...`.
3. Re-check pane titles with `panes --json`.
4. Confirm permissions were granted to the plugin and pane request.

If interactions go to the wrong place:

1. Check pane title drift/renames.
2. Re-query `panes --json` before sending input.

If command help output is unexpected:

1. `zellij pipe --name quill -- 'help'` returns the supported quill command set.
2. Some `<command> --help` invocations may not behave like a standard CLI help screen in pipe mode.
3. Prefer documented workflow examples in this skill and verify behavior with a minimal repro when needed.

## Documentation Mismatch Escalation

When observed plugin behavior does not match documented behavior:

1. Capture a minimal reproduction command and the exact observed output/error.
2. Ask the user: "Do you want me to generate a `BUGREPORT.md` file with repro steps and diagnostics?"
3. Generate `BUGREPORT.md` only if the user confirms.
4. Include in the bug report: expected behavior from docs, actual behavior, repro commands, environment details (`zellij --version`, session context, plugin wasm path), and relevant logs/errors.
