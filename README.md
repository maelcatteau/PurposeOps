# PurposeOps (`ppo`)

A personal fleet-management CLI for operating a small set of Docker-hosted customer
deployments (mostly Odoo instances) across multiple remote VPS hosts over SSH — think
`kubectl`'s "current context" model, applied to a handful of hand-run VPSes instead of a
Kubernetes cluster.

## Status

PurposeOps started as a Nushell module and has since been rewritten in Rust (`ppo-rs/`,
binary `ppo`). The rewrite is feature-complete and is the daily driver: it's installed
via `cargo install`, the shell no longer loads the nu module, and the Starship prompt
hook calls the Rust binary directly. The original nu module still lives in the repo as a
reference but isn't loaded anymore.

- [`PORTING.md`](PORTING.md) — the full phase-by-phase build/verification log (in French).
  Read this for *why* something works the way it does, or what was checked before it
  shipped.
- [`CLAUDE.md`](CLAUDE.md) — repo conventions and gotchas for anyone (human or AI)
  working on the code.

## Install

```sh
cd ppo-rs
cargo install --path .
```

This puts `ppo` in `~/.cargo/bin`. Shell completions (bash/zsh/fish/elvish/powershell/
nushell) are generated from the actual command definitions, not hand-maintained:

```sh
ppo completions nushell > /path/to/autoload/ppo-completions.nu   # or your shell of choice
```

## Quick start

```sh
ppo lsh              # list configured hosts
ppo sh mcm            # select a host
ppo sc Mael            # select a customer
ppo sd odoo-perso      # select one of that customer's deployments
ppo dps                # docker ps on the currently selected host
ppo backup run          # back up the currently selected deployment
```

Most commands operate on whatever host/customer/deployment is currently selected
(`context.yaml`), the same way `kubectl` operates on the current context.

## Commands

Grouped by area; short aliases are what you'll actually type day to day.

**Context — host / customer / deployment selection**
| Alias | Does |
|---|---|
| `h` / `hname` / `lsh` | current host info / name / list all hosts |
| `sh [id]` | select a host (fuzzy menu if no id given) |
| `ch` / `dh` | create / delete a host |
| `c` / `cname` / `lsc` | current customer info / name / list all |
| `sc [name]` | select a customer |
| `cc` / `dc` | create / delete a customer |
| `pde` / `pdei` / `lsd` | current deployment id / full record / list for current customer |
| `sd [id]` | select a deployment |
| `cdep` / `ddep` | create / delete a deployment |
| `lss` / `cs` / `ds` | list / create / delete services (deployment templates) |

**Docker** (on whatever host is currently selected)
| Alias | Does |
|---|---|
| `dps [filter] [--ports]` | container status (regex filter optional) |
| `dnls [filter]` | list Docker networks |
| `dstart` / `dstop` / `drestart` | start/stop/restart a container (fuzzy pick) |
| `dnextract` | extract a container's network info |

**SSH connections**
| Alias | Does |
|---|---|
| `close` / `closeall` / `lsconn` | close current / all master connections, list active ones |

**Backup / restore**
| Command | Does |
|---|---|
| `backup run [--cron] [--output-dir DIR]` | dump DB + filestore for the current deployment |
| `backup restore [file] [--target-database DB] [--force]` | restore an archive (destructive — drops the target DB) |

**Provisioning**
| Command | Does |
|---|---|
| `template render <service> <name>` | render a service template to a compose file, no deployment |
| `provision` | full wizard: render → push to host → `docker compose up -d` → register the deployment |

**Secrets** (credentials encrypted at rest — see Architecture below)
| Command | Does |
|---|---|
| `secrets encrypt` | encrypt any still-plaintext DB passwords / host SSH keys in the config |

**Misc**
| Command | Does |
|---|---|
| `check` | validate the whole config is internally consistent |
| `prompt` | print the Starship prompt segment for the current context |
| `t` (toggle-prompt) | toggle the prompt segment on/off |
| `completions <shell>` | print a shell-completion script |

Full `--help` (including flags) is always the source of truth: `ppo --help`, `ppo <command> --help`.

## Architecture

- **Context** (`context.yaml`): the currently selected host/customer/deployment, read by
  nearly every command. Selecting a deployment stores its *full record*, not just an id.
- **Config** (`PurposeOps-config/`, a separate git submodule): `hosts.yaml`,
  `customers.yaml` (customers own their deployments), `services.yaml` (provisioning
  templates), `context.yaml`. A customer's deployments and a host's tenants aren't a
  strict 1:1 mapping — one host can serve several customers.
- **Remote execution**: everything that touches a remote host goes through a persistent
  SSH ControlMaster connection (one multiplexed socket per host, under `controlmasters/`),
  reused across commands rather than reconnecting each time.
- **Secrets at rest**: DB passwords and host SSH keys are encrypted with
  [`age`](https://github.com/FiloSottile/age), one identity per customer
  (`~/.config/ppo/keys/<customer>.txt`). A DB password is encrypted to its owning
  customer only; a shared host's SSH key is encrypted to the union of every customer
  with a deployment there. `ppo` decrypts transparently at the point a secret is
  actually used — commands that don't touch secrets (`lsh`, `lsc`, `check`, ...) work
  fine even without any local key.
- **Provisioning**: `ppo provision` renders a `templates/<Service>/` compose file,
  pushes it to the target host (base64-embedded over the existing SSH connection — no
  scp/rsync dependency), brings the stack up, and registers the deployment.

## Development

From `ppo-rs/`:

```sh
cargo build             # or cargo check for a quick syntax pass
cargo test               # pure-logic unit tests (quoting, YAML round-trips, crypto, ...)
cargo clippy --all-targets
```

Anything that touches a remote host, Docker, or an interactive prompt is verified
**live** against real infrastructure — there's no CI. The one repeatable check beyond
`cargo test` is:

```sh
python3 tests/integration_workflow.py
```

A `pexpect`-driven end-to-end run (create host/customer/deployment → backup → simulate
data loss → restore → verify → delete everything) against the local `demo-odoo` Docker
containers. It snapshots and restores `PurposeOps-config/*.yaml` around itself and
cleans up even on failure, so it's safe to run against the real config. **Keep it
current**: per `CLAUDE.md`, any change touching a command it exercises should update the
script and get a run before the work is considered done.

Tests live in a sibling `tests.rs` file next to the module they cover (e.g.
`src/backup.rs` + `src/backup/tests.rs`), never inlined — see `CLAUDE.md` for the exact
convention.

## Roadmap

Phases 1–9 (nu→Rust parity, the cutover, secrets encryption, provisioning) are done.
Remaining, independent, any order — see `PORTING.md` for details:

- **10** — fleet status view (`ppo fleet status`: containers/uptime/disk across every host at once)
- **11** — automated backups (rotation/retention, scheduled `backup all`)
- **12** — TUI (`ratatui`) over the same CLI core
