# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`PurposeOps` (CLI alias `ppo`) is a personal Rust CLI for operating a small fleet of Docker-hosted
customer deployments (mostly Odoo instances) across multiple remote VPS hosts over SSH. The crate
lives at the repo root (`Cargo.toml`, `src/`, `tests/`) — this used to be a Nushell module, and started
life as a Rust rewrite living alongside it in `ppo-rs/`; the rewrite reached full parity, became the
daily driver, and was then promoted to the repo root with the original nu module moved to
[`archive/`](archive/) for reference only (see PORTING.md's "Réorganisation du dépôt" entry, and the
"Archived Nushell module" section below). Full step-by-step build/verification log in `PORTING.md`.

The binary is `ppo`, installed via `cargo install --path .` to `~/.cargo/bin/ppo` — it's the daily
driver (cutover done in Phase 7 of `PORTING.md`). `~/.config/nushell/config.nu` no longer loads any nu
module, and `~/.config/starship.toml`'s `[custom.ppo_context]` calls `~/.cargo/bin/ppo prompt` directly.

## Commands

- **Build / test / run**: from the repo root, `cargo build` · `cargo test` · `cargo run -- <cmd>`.
  Syntax-check equivalent for a single file is `cargo check`. After installing a new build,
  `cargo install --path .` again to update `~/.cargo/bin/ppo`.
- **Tests go in separate files — never inline `#[cfg(test)] mod tests { … }` blocks in a source
  file.** Each module `src/<mod>.rs` declares `#[cfg(test)] mod tests;` at the bottom, and the tests
  live in the sibling file `src/<mod>/tests.rs` (with `use super::*;` to reach private items). A file
  module `foo.rs` may coexist with a `foo/` directory for its submodules (2018+ edition) — that's how
  `config.rs` + `config/tests.rs` and `prompt.rs` + `prompt/tests.rs` are laid out.
- **Progress: Phases 1–11 done** (prompt, config CRUD, SSH ControlMaster, Docker, backup/restore,
  shell completions, the cutover, secrets-at-rest encryption, provisioning, host bootstrap, and a
  self-hosted backup agent with cron + ntfy alerting — see `PORTING.md` for what was verified at
  each step). Cross-arch agent builds (Phase 11.6) are deliberately deferred: spec'd but not
  automated, since every host in the fleet today is `x86_64`. Phase 12 (TUI) is independent and
  unstarted.
- `cargo test` covers only pure logic (quoting, YAML round-trips, prompt formatting, `age`
  encrypt/decrypt) and has no dependency on real infrastructure — CI (`.github/workflows/ci.yml`)
  runs `cargo build`/`test`/`clippy` on every push/PR to `master`/`rust`. Anything touching a remote
  host, Docker, or an interactive `inquire` prompt is still verified **live** against real infra, by
  hand — the two scripts below are the only repeatable live checks, and are deliberately local-only
  (not run in CI).
- **`tests/integration_workflow.py`**: a `pexpect`-driven live integration test covering the
  full lifecycle — create host/customer/deployment, `backup run`, simulate data loss, `backup
  restore`, verify, `ddep`, then delete the customer/host. Not a `cargo test` target (interactive
  wizards need a real PTY); run directly with `python3 tests/integration_workflow.py`. Requires the
  local `odoo-demo`/`odoo-demo-db` Docker containers running (see
  `~/dev/demo-odoo/docker-compose.yml`). Snapshots and restores `PurposeOps-config/*.yaml` around
  itself and cleans up scratch objects even on failure — safe to run against the real config.
  **Keep this test current**: whenever a change touches a command it exercises (`ch`/`cc`/`cdep`/
  `ddep`/`dc`/`dh`, `backup run`/`backup restore`, or anything upstream those depend on — config
  schema, `secrets.rs`, `ssh.rs`), update the script alongside the change, and run it (`cargo test`
  too) before considering the work done — the same way `cargo test`/`cargo clippy` already get run
  as a matter of course. Treat it as part of the verification pass, not an optional extra.
- **`tests/bootstrap_workflow.py`**: same idea, for `ppo bootstrap` (installing Docker/
  Nushell/Caddy/Netdata on a host — see PORTING.md Phase 10.2). A container host would be
  misleading here (Docker-in-Docker and systemd-in-a-container are both fragile in ways unrelated
  to `bootstrap.rs` itself), so this drives a real VirtualBox VM instead, reverted to a `clean`
  cloud-init snapshot before every run. One-time setup: `tests/vm/setup.sh` (downloads an Ubuntu
  24.04 cloud image, provisions it, snapshots it — slow, not part of the repeatable test). Then
  `python3 tests/bootstrap_workflow.py` reverts, boots, runs `ppo bootstrap`, verifies each
  capability actually works over a direct SSH connection (not through `ppo`), checks a second
  `ppo bootstrap` run installs nothing (idempotency), and cleans up. Same currency rule as
  `integration_workflow.py`: update and run it whenever a change touches `bootstrap.rs` or
  `ssh::exec_shell`/`exec_shell_checked`. Note: `VBoxManage snapshot restore` reverts machine
  settings (e.g. `--uartmode1`) as well as disk state, not just the disk — apply any one-off
  `modifyvm` change *after* a revert, not before, or it's silently undone.
- **`tests/backup_agent_workflow.py`**: same idea, for `ppo backup bootstrap-agent` (installing a
  self-contained `ppo` agent + local cron on a deployment's own host — see PORTING.md Phase 11).
  Reuses `tests/vm/` rather than a third harness. Verifies the pushed binary runs, the generated
  `/etc/cron.d/...` entry and scoped config are correct, a directly-triggered `backup run --cron`
  actually produces an archive, a deliberately broken run reports a nonzero exit **and** that the
  failure notification actually reaches a real ntfy topic (polled, not just trusted), and that
  re-running `bootstrap-agent` replaces the cron file rather than duplicating it. Same currency
  rule: update and run it whenever a change touches `backup_agent.rs`, `provision.rs`'s
  `push_file`/`push_binary`, `secrets.rs`'s agent-identity functions, or `backup.rs`'s retention/
  `notify_failure` additions.

## Archived Nushell module (`archive/`, reference only)

Everything in this section describes the **original nu module**, not the active Rust code — it's kept
because `archive/` is retained as a fallback/reference (see "What this is" above), not because anyone
is expected to extend it. If you're working on the Rust CLI, this section isn't the one you want; there
isn't yet a consolidated "Rust architecture" writeup in this file beyond the bullet points under
Commands and the per-phase detail in `PORTING.md` — read `src/` directly plus `PORTING.md`'s phase
entries for how the current code is organized.

- **Syntax-check a module** (the closest thing `archive/` has to a build/lint step):
  `nu -c "nu-check <path/to/file.nu>"`
- **Load the module fresh and run a command** (bypasses any already-loaded/stale definitions in an
  interactive shell — see Gotchas):
  `nu --no-config-file -c "use /home/ngner/dev/nu-modules/PurposeOps/archive/ppo.nu; ppo <command>"`
- No automated tests exist for this code, and none are planned — it's frozen. Historically (when it
  was the active codebase, loaded via `~/.config/nushell/config.nu`), verifying a change meant running
  the actual `ppo <command>` against real (or scratch) host/customer config and inspecting output/side
  effects on the remote host.

### Module layout convention

Every subsystem is a directory with a `mod.nu` that does `export use core.nu *` (plus `internal.nu`,
`validations.nu` as needed) and defines short aliases at the bottom (e.g. `export alias "sc" =
set-customer`). `ppo.nu` re-exports every subsystem module and used to be the single entry
point loaded by the user's shell config. Within a subsystem, `core.nu` holds the public/interactive
commands, `internal.nu` holds the internal write/mutation helpers, `validations.nu` holds pure
consistency checks — this split is intentional and was carried over into the Rust rewrite's own
module conventions (see the "Tests go in separate files" bullet under Commands).

### The context: a single "current selection" state

`archive/context/context-manager.nu` reads/writes `context.yaml` (path from `archive/config/config.nu`, actual file
lives in the `PurposeOps-config` submodule). This file holds the *currently selected* host, customer,
and deployment — effectively global state for the interactive session, analogous to `kubectl`'s current
context. Nearly every command starts by calling `load_context` and reading `ctx.customer` /
`ctx.deployment` / `ctx.host`. `archive/deployment-manager` stores the **full deployment record** (not just an
id) in the context when you `sd` (set-deployment); `get-current-deployment-info` errors out if it finds
the old string-id-only format, since that used to be the schema before a migration.

### Config data lives in a separate private submodule

`archive/config/config.nu` only defines path constants (`hosts_config_path`, `customers_config_path`,
`services_config_path`, `context_path`) — they all point into `PurposeOps-config/`, which is a **separate
git submodule/repo** (`PurposeOps-config.git`) holding the actual YAML data (hosts, customers,
deployments, DB credentials, services) and is shared with the active Rust code (`src/config.rs`'s
`base_path()` points at the same submodule). Changes to data (adding a host/customer/service) need a
commit in that submodule too, regardless of which codebase made the change.

### Creating config entries (customer/host/service/deployment)

`archive/config/` has the CRUD-creation trio for top-level config objects: `create_customer` (`cc`),
`create_host` (`ch`), `create_service` (`cs`) — each interactively prompts, previews the record as YAML,
and on confirmation does `open $x_config_path | insert <name> <record> | save $x_config_path -f`.
`archive/deployment-manager/core.nu` follows the same pattern for `create_deployment` (`cdep`), except a
deployment isn't top-level — it appends into the **currently selected customer's** `deployments` list
(`sc <customer>` first), via `create_deployment_internal` in `internal.nu`. It validates the host exists
and that the new `deployment_id` isn't already used by *any* customer (ids are looked up globally, e.g.
by `host_for_deployment`), and only prompts for the DB-backup fields (`container_name`,
`db_container_name`, `database_name`, `db_credentials`) if you say the deployment has a database —
those fields are what `archive/customer-manager/backup.nu` requires, so skip them for non-DB services like
Vaultwarden/Caddy.

Any `open | insert | save` round-trip on a YAML config file re-serializes the **whole file** in
Nushell's own style: it strips inline comments, normalizes indentation, and switches quoting (e.g.
`"5432"` → `'5432'`). No data loss, but don't be surprised if a `create_*` command reformats unrelated
parts of the file or drops a comment you'd left in `customers.yaml`.

### Remote execution model

Everything that touches a remote host goes through `archive/ssh-manager.nu`'s SSH **ControlMaster**
connection (`run_with_master`): a persistent multiplexed SSH connection per host is kept alive under
`controlmasters/`, and individual commands are sent as `ssh -S <socket> ... <target> "<command string>"`.
`archive/docker/core.nu`'s `run_docker_command [command: list, host_info: record]` is the standard way to
run a docker command against a given host — for `localhost` it uses `run-external "docker" ...`, for
remote hosts it builds a single shell command string and sends it over the ControlMaster connection.
**Every argument is individually shell-quoted** before joining (via a local `shell-quote` helper) — this
matters because any unquoted argument containing spaces (e.g. a `sh -c "multi word script"` payload)
gets split apart by the remote shell and silently mangled. The Rust code (`src/ssh.rs`,
`src/docker.rs`) follows the same model: a shared `run_with_master`, and its own `shell-quote`
equivalent for argument quoting.

`archive/customer-manager/backup.nu` has module-level `exec-remote` / `exec-remote-checked` (docker
commands) and `exec-remote-shell` / `exec-remote-shell-checked` (raw host shell commands) helpers, each
taking `host_info: record` as an explicit parameter. `resolve-remote-path` in the same file handles the
`~` gotcha below.

`archive/customer-manager/backup.nu` also has `backup restore` (engine: `do-generic-restore`), which restores a `backup run`
archive into the **currently selected deployment** — a backup can come from a totally different
customer/deployment (the archive's `.sql`/`_fs.tar.gz` filenames encode the *source* database name, not
the target), so restore resolves the target database/containers from context same as backup does, but
takes the backup file as an explicit argument. It's destructive (`DROP DATABASE` on the target) and asks
for confirmation unless `--force` is passed. `src/backup.rs`'s `cmd_backup_restore`/`do_generic_restore`
port this behavior faithfully.

### `docker-compose-functions.nu` is correct but not wired in

`archive/docker-compose-functions.nu` (compose start/stop/restart for a single service, picked
interactively) was updated to match the `archive/docker/core.nu` API (`with_host_info` +
`get_containers` + `select_container`, `host_info` threaded through `run_docker_command`), same pattern
as `archive/docker/operations.nu`'s `docker_container_operation`. It was never exported from `ppo.nu`
though — `docker_compose_stack_operation` was unreachable from the `ppo` CLI even before the nu module
was archived, and stayed that way; there's no Rust equivalent either. Left as-is; port/wire it in if it
turns out to be needed.

### Nu-specific gotchas (only relevant if you're touching `archive/`)

- **Unescaped parentheses in Nushell string interpolation are command calls, not literal text.**
  `$"... (depuis ($x)) ..."` does not print a literal `(depuis ...)` — the outer unescaped `(` starts a
  command-substitution, so Nushell tries to run a command literally named `depuis` and fails with
  `External command failed`/`command not found`. Always escape literal parens in `$"..."` as `\(` `\)`
  (see the working examples in `archive/customer-manager/backup.nu`, e.g. `\(code ($result.exit_code)\)`). This
  bites hardest when building a *remote shell script as a Nushell string* and you want a literal bash
  `$(...)` command substitution in it (e.g. `SRC_DIR=$(ls ...)`) — the `$` doesn't protect the `(` from
  Nushell's own interpolation, so write `$\(ls ...\)` to get a literal `$(ls ...)` in the resulting
  string for the remote shell to interpret. Otherwise Nushell runs `ls` itself, immediately, against
  whatever literal (unexpanded) path is inside — a `Not found` error with no further context.
- **`| complete` only suppresses "non-zero exit code" errors, not "failed to spawn" errors**, and only for
  the external command syntactically most-recently piped to it — errors raised deeper inside a nested
  `def` still propagate as normal Nushell errors up through `try`/`catch`. Don't assume `| complete`
  makes a remote command call silent-safe by itself.

## General gotchas (apply to the Rust code too, not just `archive/`)

- **A `~` inside a remote path only expands if the shell actually sees it unquoted.** Any path built for
  a remote command that gets wrapped in single quotes (needed to keep it one shell word) will NOT have
  its `~` expanded by the remote shell — single quotes suppress tilde-expansion same as they suppress
  variable expansion. `| path expand` doesn't help either since that resolves the *local* laptop's `~`,
  not the remote SSH user's. The nu module used `resolve-remote-path` in `archive/customer-manager/backup.nu`
  (replaces `~` with a hardcoded `/home/ngner`); the Rust code has the same function, same fix, in
  `src/backup.rs`. Use it (or its equivalent) before embedding a `~/...` path in any remote command
  string.
- **`docker exec` requires the container to be running; `docker cp` doesn't.** If a workflow needs to
  stop a container (e.g. to release DB connections before a `DROP DATABASE`), anything that needs to
  touch files *inside* that container in the meantime has to go through `docker cp` (host ⇄ container,
  works on a stopped container) rather than `docker exec ... sh -c ...`, which fails with "container ...
  is not running". `backup restore`'s filestore-restore step (both `archive/customer-manager/backup.nu`
  and `src/backup.rs`) does the tar extraction on the **host** shell and only uses `docker cp` while the
  app container is stopped, saving `docker exec`-only work (like `chown`) for after it's restarted.
- **`docker cp` and `docker exec` can disagree on file ownership.** Files copied into a container via
  `docker cp` land owned by whatever UID performed the copy, which may not match the container's default
  `docker exec` user — a plain `rm` cleanup can fail with "Operation not permitted" *silently* if the
  call isn't checking the exit code. Use `docker exec -u root ...` for cleanup of files that weren't
  created by the container's own process.
