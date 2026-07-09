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

### Creating config entries (customer/host/service/deployment)

`config/` has the CRUD-creation trio for top-level config objects: `create_customer` (`cc`),
`create_host` (`ch`), `create_service` (`cs`) — each interactively prompts, previews the record as YAML,
and on confirmation does `open $x_config_path | insert <name> <record> | save $x_config_path -f`.
`deployment-manager/core.nu` follows the same pattern for `create_deployment` (`cdep`), except a
deployment isn't top-level — it appends into the **currently selected customer's** `deployments` list
(`sc <customer>` first), via `create_deployment_internal` in `internal.nu`. It validates the host exists
and that the new `deployment_id` isn't already used by *any* customer (ids are looked up globally, e.g.
by `host_for_deployment`), and only prompts for the DB-backup fields (`container_name`,
`db_container_name`, `database_name`, `db_credentials`) if you say the deployment has a database —
those fields are what `customer-manager/backup.nu` requires, so skip them for non-DB services like
Vaultwarden/Caddy.

Any `open | insert | save` round-trip on a YAML config file re-serializes the **whole file** in
Nushell's own style: it strips inline comments, normalizes indentation, and switches quoting (e.g.
`"5432"` → `'5432'`). No data loss, but don't be surprised if a `create_*` command reformats unrelated
parts of the file or drops a comment you'd left in `customers.yaml`.

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
`run_docker_command`, or if you need a raw (non-`docker`) remote shell command, use `run_with_master`
directly rather than shelling out locally.

`customer-manager/backup.nu` has module-level `exec-remote` / `exec-remote-checked` (docker commands)
and `exec-remote-shell` / `exec-remote-shell-checked` (raw host shell commands) helpers, each taking
`host_info: record` as an explicit parameter — reuse these for any new backup/restore-adjacent remote
work rather than re-deriving the same error-checking boilerplate. `resolve-remote-path` in the same file
handles the `~` gotcha below and should be used any time a path is built for a remote command.

`backup.nu` also has `backup restore` (engine: `do-generic-restore`), which restores a `backup run`
archive into the **currently selected deployment** — a backup can come from a totally different
customer/deployment (the archive's `.sql`/`_fs.tar.gz` filenames encode the *source* database name, not
the target), so restore resolves the target database/containers from context same as backup does, but
takes the backup file as an explicit argument. It's destructive (`DROP DATABASE` on the target) and asks
for confirmation unless `--force` is passed.

### `docker-compose-functions.nu` is correct but not wired in

`docker-compose-functions.nu` (compose start/stop/restart for a single service, picked interactively)
has been updated to match the current `docker/core.nu` API (`with_host_info` + `get_containers` +
`select_container`, `host_info` threaded through `run_docker_command`), same pattern as
`docker/operations.nu`'s `docker_container_operation`. It is **not** exported from `ppo.nu` though —
there's no `export use docker-compose-functions.nu *` line, so `docker_compose_stack_operation` is
currently unreachable from the `ppo` CLI. This was left alone deliberately; wire it in (with aliases,
following the `dstop`/`dstart`/`drestart` pattern in `docker/mod.nu`) if/when it's actually needed.

Note `docker/mod.nu` only re-exports `operations.nu *` and `status.nu *` — `core.nu` and `ui.nu` are
not re-exported, so anything outside `docker/` that needs `run_docker_command`, `get_containers`,
`select_container`, etc. must `use docker/core.nu *` / `use docker/ui.nu *` directly rather than
`use docker/ *`.

## Gotchas specific to this codebase (learned from real bugs, not theoretical)

- **Unescaped parentheses in Nushell string interpolation are command calls, not literal text.**
  `$"... (depuis ($x)) ..."` does not print a literal `(depuis ...)` — the outer unescaped `(` starts a
  command-substitution, so Nushell tries to run a command literally named `depuis` and fails with
  `External command failed`/`command not found`. Always escape literal parens in `$"..."` as `\(` `\)`
  (see the working examples in `customer-manager/backup.nu`, e.g. `\(code ($result.exit_code)\)`). This
  bites hardest when building a *remote shell script as a Nushell string* and you want a literal bash
  `$(...)` command substitution in it (e.g. `SRC_DIR=$(ls ...)`) — the `$` doesn't protect the `(` from
  Nushell's own interpolation, so write `$\(ls ...\)` to get a literal `$(ls ...)` in the resulting
  string for the remote shell to interpret. Otherwise Nushell runs `ls` itself, immediately, against
  whatever literal (unexpanded) path is inside — a `Not found` error with no further context.
- **A `~` inside a remote path only expands if the shell actually sees it unquoted.** Any path built for
  a remote command that gets wrapped in single quotes (needed to keep it one shell word) will NOT have
  its `~` expanded by the remote shell — single quotes suppress tilde-expansion same as they suppress
  variable expansion. `| path expand` doesn't help either since that resolves the *local* laptop's `~`,
  not the remote SSH user's. Use `resolve-remote-path` in `customer-manager/backup.nu` (replaces `~` with
  a hardcoded `/home/ngner`) before embedding a `~/...` path in any remote command string.
- **`docker exec` requires the container to be running; `docker cp` doesn't.** If a workflow needs to
  stop a container (e.g. to release DB connections before a `DROP DATABASE`), anything that needs to
  touch files *inside* that container in the meantime has to go through `docker cp` (host ⇄ container,
  works on a stopped container) rather than `docker exec ... sh -c ...`, which fails with "container ...
  is not running". `backup restore`'s filestore-restore step does the tar extraction on the **host**
  shell and only uses `docker cp` while the app container is stopped, saving `docker exec`-only work
  (like `chown`) for after it's restarted.
- **`docker cp` and `docker exec` can disagree on file ownership.** Files copied into a container via
  `docker cp` land owned by whatever UID performed the copy, which may not match the container's default
  `docker exec` user — a plain `rm` cleanup can fail with "Operation not permitted" *silently* if the
  call isn't checking `exit_code` (many are, via `| complete` without a follow-up check). Use `docker exec
  -u root ...` for cleanup of files that weren't created by the container's own process.
- **`| complete` only suppresses "non-zero exit code" errors, not "failed to spawn" errors**, and only for
  the external command syntactically most-recently piped to it — errors raised deeper inside a nested
  `def` still propagate as normal Nushell errors up through `try`/`catch`. Don't assume `| complete`
  makes a remote command call silent-safe by itself.
