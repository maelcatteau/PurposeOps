# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`PurposeOps` (CLI alias `ppo`) is a personal Nushell module for operating a small fleet of Docker-hosted
customer deployments (mostly Odoo instances) across multiple remote VPS hosts over SSH. There is no
build step, no test suite, and no CI — it's a set of Nushell modules loaded directly into an interactive
shell.

## Commands

- **Syntax-check a module** (the closest thing to a build/lint step):
  `nu -c "nu-check <path/to/file.nu>"`
- **Load the module fresh and run a command** (bypasses any already-loaded/stale definitions in an
  interactive shell — see Gotchas):
  `nu --no-config-file -c "use /home/ngner/dev/nu-modules/PurposeOps/ppo.nu; ppo <command>"`
- **In an interactive `nu` shell**, the module is loaded via `~/.config/nushell/config.nu` (`use
  ~/dev/nu-modules/PurposeOps/ppo.nu`), so all commands are invoked as `ppo <command>`. Nushell parses
  `use` at the point it runs — editing a `.nu` file does **not** hot-reload an already-running shell;
  the shell must be restarted (or the module re-`use`d) to pick up changes.
- No automated tests exist. Verifying a change means running the actual `ppo <command>` against real
  (or a scratch) host/customer config and inspecting output/side effects on the remote host.

## Architecture

### Module layout convention

Every subsystem is a directory with a `mod.nu` that does `export use core.nu *` (plus `internal.nu`,
`validations.nu` as needed) and defines short aliases at the bottom (e.g. `export alias "sc" =
set-customer`). `ppo.nu` at the repo root re-exports every subsystem module and is the single entry
point loaded by the user's shell config. Within a subsystem, `core.nu` holds the public/interactive
commands, `internal.nu` holds the internal write/mutation helpers, `validations.nu` holds pure
consistency checks — this split is intentional and worth preserving when adding commands.

### The context: a single "current selection" state

`context/context-manager.nu` reads/writes `context.yaml` (path from `config/config.nu`, actual file
lives in the `PurposeOps-config` submodule). This file holds the *currently selected* host, customer,
and deployment — effectively global state for the interactive session, analogous to `kubectl`'s current
context. Nearly every command starts by calling `load_context` and reading `ctx.customer` /
`ctx.deployment` / `ctx.host`. `deployment-manager` stores the **full deployment record** (not just an
id) in the context when you `sd` (set-deployment); `get-current-deployment-info` errors out if it finds
the old string-id-only format, since that used to be the schema before a migration.

### Config data lives in a separate private submodule

`config/config.nu` only defines path constants (`hosts_config_path`, `customers_config_path`,
`services_config_path`, `context_path`) — they all point into `PurposeOps-config/`, which is a **separate
git submodule/repo** (`PurposeOps-config.git`) holding the actual YAML data (hosts, customers,
deployments, DB credentials, services). Changes to data (adding a host/customer/service) need a commit
in that submodule too. Note: the top-level `.gitignore` still lists `config/*.yaml` paths from before
this submodule split — those entries are stale/dead and don't match anything under `config/` anymore.

### Remote execution model

Everything that touches a remote host goes through `ssh-manager.nu`'s SSH **ControlMaster** connection
(`run_with_master`): a persistent multiplexed SSH connection per host is kept alive under
`controlmasters/`, and individual commands are sent as `ssh -S <socket> ... <target> "<command string>"`.
`docker/core.nu`'s `run_docker_command [command: list, host_info: record]` is the standard way to run a
docker command against a given host — for `localhost` it uses `run-external "docker" ...`, for remote
hosts it builds a single shell command string and sends it over the ControlMaster connection. **Every
argument is individually shell-quoted** before joining (via a local `shell-quote` helper) — this matters
because any unquoted argument containing spaces (e.g. a `sh -c "multi word script"` payload) gets split
apart by the remote shell and silently mangled. When adding new remote-executing code, either reuse
`run_docker_command`, or if you need a raw (non-`docker`) remote shell command, follow the
`exec-remote-shell` pattern in `customer-manager/backup.nu` (quote/build the string yourself and call
`run_with_master` directly) rather than shelling out locally.

### Known architectural inconsistency (mid-refactor)

The repo is actively being split from flat top-level files into subsystem directories (see git history:
`docker-functions.nu` → `docker/`, `deployment-manager.nu` → `deployment-manager/`, etc.). Two files have
**not** been migrated and are stale relative to the current `docker/core.nu` API:
`docker-compose-functions.nu` and `service-manager.nu`'s `docker_compose_stack_operation` still call
`run_docker_command [...]` with a single list argument (the old pre-`host_info` signature) and reference
`get_containers_list`, which no longer exists (it lived in the now-deleted `docker-functions.nu`). Treat
these as broken/pending-refactor rather than a reference implementation.

## Gotchas specific to this codebase (learned from real bugs, not theoretical)

- **Unescaped parentheses in Nushell string interpolation are command calls, not literal text.**
  `$"... (depuis ($x)) ..."` does not print a literal `(depuis ...)` — the outer unescaped `(` starts a
  command-substitution, so Nushell tries to run a command literally named `depuis` and fails with
  `External command failed`/`command not found`. Always escape literal parens in `$"..."` as `\(` `\)`
  (see the working examples in `customer-manager/backup.nu`, e.g. `\(code ($result.exit_code)\)`).
- **`docker cp` and `docker exec` can disagree on file ownership.** Files copied into a container via
  `docker cp` land owned by whatever UID performed the copy, which may not match the container's default
  `docker exec` user — a plain `rm` cleanup can fail with "Operation not permitted" *silently* if the
  call isn't checking `exit_code` (many are, via `| complete` without a follow-up check). Use `docker exec
  -u root ...` for cleanup of files that weren't created by the container's own process.
- **`| complete` only suppresses "non-zero exit code" errors, not "failed to spawn" errors**, and only for
  the external command syntactically most-recently piped to it — errors raised deeper inside a nested
  `def` still propagate as normal Nushell errors up through `try`/`catch`. Don't assume `| complete`
  makes a remote command call silent-safe by itself.
