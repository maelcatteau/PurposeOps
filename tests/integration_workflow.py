#!/usr/bin/env python3
"""Live integration test for ppo: full CRUD + backup/restore lifecycle.

Not a `cargo test` target — `cc`/`ch`/`cdep` are interactive `inquire` wizards
that need a real PTY, so this drives the real `ppo` binary through pexpect,
matching how every feature in this project has been verified by hand. See
PORTING.md for the project's "verify live, no automated test suite" convention.
Shared wizard/VM-driving boilerplate lives in `ppo_test_helpers.py`.

What it does, against real PurposeOps-config and the local `odoo-demo`/
`odoo-demo-db` Docker containers (must already be running):

  1. Snapshot hosts.yaml/customers.yaml/context.yaml/services.yaml.
  2. Create a scratch Postgres DB (`inttest_db`) with one marker row.
  3. ch: create a scratch host targeting hostname=localhost.
  4. sh: select that host directly by id (not through a wizard).
  5. cc: create a scratch customer on that host.
  6. cdep: create a deployment pointing at odoo-demo/odoo-demo-db/inttest_db.
  7. backup run: back up inttest_db.
  8. Drop the marker table (simulate data loss).
  9. backup restore --force: restore from the archive just created.
  10. Verify the marker row is back with the correct value.
  11. dstop/dstart/drestart: round trip against a dedicated scratch container.
  12. cs/lss/ds: create/list/delete a scratch service catalog entry.
  13. provision: render+push+`docker compose up -d` a scratch Vaultwarden
      deployment, verify the container is actually running, tear it down.
  14. ddep: delete the deployment directly (not via customer cascade).
  15. dc / dh: delete the now deployment-less scratch customer and host.
  16. Restore hosts.yaml/customers.yaml/context.yaml/services.yaml
      byte-for-byte, drop inttest_db, remove the scratch customer's generated
      age key, remove the backup archive(s) and scratch docker resources
      created on disk.

Cleanup (step 16) always runs, even on failure, so a crashed run doesn't leave
scratch state behind. Exit code 0 = pass, 1 = fail.

Usage: python3 tests/integration_workflow.py
Env: PPO_BIN to override the binary path (default: target/debug/ppo).
"""

import os
import sys
import traceback

import pexpect

import ppo_test_helpers as h

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PPO_BIN = os.environ.get("PPO_BIN", os.path.join(REPO_ROOT, "target", "debug", "ppo"))
CONFIG_DIR = os.path.join(REPO_ROOT, "PurposeOps-config")
KEYS_DIR = os.path.expanduser("~/.config/ppo/keys")

CUSTOMER = "IntegrationTest"
ABBREV = "inttest"
HOST_ID = "inttest-local"
DEPLOYMENT_ID = "inttest-deploy"
DB_NAME = "inttest_db"
MARKER_VALUE = "integration-test-checkpoint"
# Backups now live under the deployment's own path_for_service (see PORTING.md), not a
# centralized ~/backups/<abbrev>/<host_id> tree — must match create_deployment()'s
# "Path for service on host" value.
PATH_FOR_SERVICE = "/home/ngner/dev/demo-odoo"

LIFECYCLE_CONTAINER = "inttest-docker-lifecycle"

SCRATCH_SERVICE = "IntegrationTestService"

PROVISION_DEPLOYMENT_ID = "inttest-provision-vw"
PROVISION_SERVICE_NAME = "vw-inttest"
PROVISION_NETWORK = "inttest-provision-net"
PROVISION_DIR = "/tmp/ppo-inttest-provision"

PASS = "\033[32mPASS\033[0m"
FAIL = "\033[31mFAIL\033[0m"


def step(msg):
    print(f"\n== {msg}")


def spawn(args, timeout=15):
    return h.spawn(PPO_BIN, REPO_ROOT, args, timeout=timeout)


def run(cmd, check=True, **kw):
    return h.run(cmd, check=check, **kw)


def psql(sql, database="postgres", check=True):
    return run(
        [
            "docker", "exec", "-e", "PGPASSWORD=odoo_demo", "odoo-demo-db",
            "psql", "-h", "localhost", "-U", "odoo", "-d", database, "-tAc", sql,
        ],
        check=check,
    )


def preflight():
    step("Preflight checks")
    if not os.path.isfile(PPO_BIN):
        raise RuntimeError(f"ppo binary not found at {PPO_BIN} — run `cargo build` first")

    names = run(["docker", "ps", "--format", "{{.Names}}"]).stdout.split()
    for c in ("odoo-demo", "odoo-demo-db"):
        if c not in names:
            raise RuntimeError(f"container '{c}' is not running — start demo-odoo first")

    customers = h.read_file(os.path.join(CONFIG_DIR, "customers.yaml"))
    hosts = h.read_file(os.path.join(CONFIG_DIR, "hosts.yaml"))
    services = h.read_file(os.path.join(CONFIG_DIR, "services.yaml"))
    if f"{CUSTOMER}:" in customers or f"{HOST_ID}:" in hosts or f"{SCRATCH_SERVICE}:" in services:
        raise RuntimeError(
            f"'{CUSTOMER}', '{HOST_ID}' or '{SCRATCH_SERVICE}' already present in config — "
            "leftover from a previous failed run? Check and clean up manually."
        )
    print("ok")


def snapshot():
    return {
        "hosts.yaml": h.read_file(os.path.join(CONFIG_DIR, "hosts.yaml")),
        "customers.yaml": h.read_file(os.path.join(CONFIG_DIR, "customers.yaml")),
        "context.yaml": h.read_file(os.path.join(CONFIG_DIR, "context.yaml")),
        "services.yaml": h.read_file(os.path.join(CONFIG_DIR, "services.yaml")),
    }


def restore_snapshot(snap):
    for name, content in snap.items():
        h.write_file(os.path.join(CONFIG_DIR, name), content)


def setup_test_db():
    step(f"Creating scratch database '{DB_NAME}' with a marker row")
    psql(f'DROP DATABASE IF EXISTS "{DB_NAME}"')
    psql(f'CREATE DATABASE "{DB_NAME}" OWNER odoo')
    psql("CREATE TABLE marker (id serial primary key, value text)", database=DB_NAME)
    psql(f"INSERT INTO marker (value) VALUES ('{MARKER_VALUE}')", database=DB_NAME)
    out = psql("SELECT value FROM marker", database=DB_NAME).stdout.strip()
    assert out == MARKER_VALUE, f"setup sanity check failed: got {out!r}"
    print("ok")


def create_host():
    step(f"ch: create scratch host '{HOST_ID}'")
    h.create_host(
        PPO_BIN, REPO_ROOT,
        host_id=HOST_ID, hostname="localhost", user="ngner", port="22", identity_file="",
    )


def select_host_direct():
    step(f"sh: select host '{HOST_ID}' directly (not via a wizard)")
    run([PPO_BIN, "sh", HOST_ID], cwd=REPO_ROOT)
    result = run([PPO_BIN, "hname"], cwd=REPO_ROOT)
    assert result.stdout.strip() == HOST_ID, f"hname mismatch after sh: {result.stdout!r}"
    print("ok")


def create_customer():
    step(f"cc: create scratch customer '{CUSTOMER}'")
    h.create_customer(
        PPO_BIN, REPO_ROOT,
        name=CUSTOMER, abbrev=ABBREV, host_id=HOST_ID, path_on_host="/home/ngner/dev/demo-odoo",
    )


def select_customer():
    run([PPO_BIN, "sc", CUSTOMER], cwd=REPO_ROOT)


def create_deployment():
    step(f"cdep: create deployment '{DEPLOYMENT_ID}'")
    child = spawn("cdep")
    child.expect("Service name")
    child.sendline("Integration Test Service")
    child.expect("Host ID")
    child.sendline(HOST_ID)
    child.expect("Path for service on host")
    child.sendline(PATH_FOR_SERVICE)
    child.expect("Path for docker-compose file")
    child.sendline("/home/ngner/dev/demo-odoo/docker-compose.yml")
    child.expect("Deployment id")
    child.sendline(DEPLOYMENT_ID)
    child.expect("base de donn")
    child.sendline("y")
    child.expect("Container name")
    child.sendline("odoo-demo")
    child.expect("DB container name")
    child.sendline("odoo-demo-db")
    child.expect("Database name")
    child.sendline(DB_NAME)
    child.expect("DB credentials - host")
    child.sendline("odoo-demo-db")
    child.expect("DB credentials - port")
    child.sendline("5432")
    child.expect("DB credentials - user")
    child.sendline("odoo")
    child.expect("DB credentials - password")
    child.sendline("odoo_demo")
    child.expect("ce d.*ploiement")
    child.sendline("y")
    child.expect(pexpect.EOF, timeout=15)
    child.close()
    assert child.exitstatus == 0, "cdep failed"

    customers = h.read_file(os.path.join(CONFIG_DIR, "customers.yaml"))
    assert "enc:" in customers, "db_credentials.password was not encrypted by cdep"


def select_deployment():
    run([PPO_BIN, "sd", DEPLOYMENT_ID], cwd=REPO_ROOT)


def backup_run():
    step("backup run")
    result = run([PPO_BIN, "backup", "run"], cwd=REPO_ROOT)
    print(result.stdout)
    for line in result.stdout.splitlines():
        if "Succès" in line and "disponible sur le serveur" in line:
            path = line.split(":", 1)[1].strip()
            print(f"archive: {path}")
            return path
    raise RuntimeError("backup run did not report a success path")


def sabotage():
    step("Dropping marker table to simulate data loss")
    psql("DROP TABLE marker", database=DB_NAME)
    out = psql("SELECT to_regclass('marker')", database=DB_NAME).stdout.strip()
    assert out in ("", "\\N", "None"), f"marker table still present: {out!r}"
    print("ok, marker table gone")


def backup_restore(archive_path):
    step("backup restore --force")
    result = run([PPO_BIN, "backup", "restore", archive_path, "--force"], cwd=REPO_ROOT)
    print(result.stdout)
    assert "Succès" in result.stdout, "backup restore did not report success"


def verify_restored():
    step("Verifying restored data")
    out = psql("SELECT value FROM marker", database=DB_NAME).stdout.strip()
    assert out == MARKER_VALUE, f"restored marker value mismatch: got {out!r}"
    print(f"ok: marker value = {out!r}")


def container_running(name):
    return run(["docker", "inspect", "-f", "{{.State.Running}}", name]).stdout.strip() == "true"


def rm_rf_via_docker(path):
    # Vaultwarden (and most app images) run as root inside the container, so files it
    # writes into a bind-mounted volume land root-owned on the host — a plain `rm -rf`
    # as our own unprivileged user silently fails (no write permission on that
    # subdirectory) and leaves scratch data behind. Remove it the same way it was
    # created: as root, inside a throwaway container.
    if os.path.isdir(path):
        # `rm -rf /cleanup` itself exits non-zero — it can empty the bind-mounted
        # directory but not unlink the mountpoint entry itself ("Resource busy"),
        # which is expected and harmless here (the container is `--rm`ed right after).
        run(
            ["docker", "run", "--rm", "-v", f"{path}:/cleanup", "alpine:3.20", "rm", "-rf", "/cleanup"],
            check=False,
        )
    run(["rm", "-rf", path], check=False)


def docker_lifecycle_roundtrip():
    step(f"dstop/dstart/drestart: round trip against scratch container '{LIFECYCLE_CONTAINER}'")
    # A dedicated, uniquely-named scratch container — not odoo-demo/odoo-demo-db — so the
    # fuzzy-select prompt has exactly one candidate and typing its name can't accidentally
    # match a similarly-named container (there's more than one odoo-demo* on this box).
    run(["docker", "rm", "-f", LIFECYCLE_CONTAINER], check=False)
    run(["docker", "run", "-d", "--name", LIFECYCLE_CONTAINER, "alpine:3.20", "sleep", "infinity"])
    assert container_running(LIFECYCLE_CONTAINER), "scratch container did not start"

    child = spawn("dstop")
    child.expect("Select a container to stop")
    child.send(LIFECYCLE_CONTAINER)
    child.sendline("")
    child.expect("stopped successfully")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "dstop failed"
    assert not container_running(LIFECYCLE_CONTAINER), "container still running after dstop"

    child = spawn("dstart")
    child.expect("Select a container to start")
    child.send(LIFECYCLE_CONTAINER)
    child.sendline("")
    child.expect("started successfully")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "dstart failed"
    assert container_running(LIFECYCLE_CONTAINER), "container not running after dstart"

    child = spawn("drestart")
    child.expect("Select a container to restart")
    child.send(LIFECYCLE_CONTAINER)
    child.sendline("")
    child.expect("restarted successfully")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "drestart failed"
    assert container_running(LIFECYCLE_CONTAINER), "container not running after drestart"
    print("ok: dstop/dstart/drestart all confirmed against real container state")


def service_catalog_roundtrip():
    step(f"cs/lss/ds: scratch service catalog entry '{SCRATCH_SERVICE}'")
    child = spawn("cs")
    child.expect("Service name")
    child.sendline(SCRATCH_SERVICE)
    child.expect("Template directory path")
    child.sendline("~/dev/nu-modules/PurposeOps/templates/IntegrationTestService/")
    child.expect("Template docker compose path")
    child.sendline("~/dev/nu-modules/PurposeOps/templates/IntegrationTestService/docker-compose.yml")
    child.expect("Create")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "cs failed"

    result = run([PPO_BIN, "lss"], cwd=REPO_ROOT)
    assert SCRATCH_SERVICE in result.stdout, f"lss did not list {SCRATCH_SERVICE}: {result.stdout!r}"

    child = spawn("ds")
    child.expect("Select service")
    child.send(SCRATCH_SERVICE)
    child.sendline("")
    child.expect("Delete")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "ds failed"

    result = run([PPO_BIN, "lss"], cwd=REPO_ROOT)
    assert SCRATCH_SERVICE not in result.stdout, f"lss still lists {SCRATCH_SERVICE} after ds: {result.stdout!r}"
    print("ok")


def provision_roundtrip():
    step("provision: Vaultwarden round trip (render -> push -> docker compose up -d -> verify)")
    run(["docker", "rm", "-f", PROVISION_SERVICE_NAME], check=False)
    run(["docker", "network", "rm", PROVISION_NETWORK], check=False)
    run(["docker", "network", "create", PROVISION_NETWORK])
    rm_rf_via_docker(PROVISION_DIR)

    child = spawn("provision", timeout=30)
    child.expect("Service")
    child.send("Vaultwarden")
    child.sendline("")
    child.expect("Host ID")
    child.send(HOST_ID)
    child.sendline("")
    child.expect("Nom du service Docker")
    child.sendline(PROVISION_SERVICE_NAME)
    child.expect("Chemin du service sur")
    child.sendline(PROVISION_DIR)
    child.expect("Chemin du fichier docker-compose")
    child.sendline(f"{PROVISION_DIR}/docker-compose.yml")
    child.expect("Deployment id")
    child.sendline(PROVISION_DEPLOYMENT_ID)
    child.expect("Data path on host")
    child.sendline(f"{PROVISION_DIR}/data")
    child.expect("exposed behind your reverse proxy")
    child.sendline("8080")
    child.expect("network used to link")
    child.sendline(PROVISION_NETWORK)
    child.expect("Domain for Vaultwarden")
    child.sendline("https://vault.inttest.local")
    child.expect("Provisionner ce service")
    child.sendline("y")
    child.expect(pexpect.EOF, timeout=60)
    child.close()
    assert child.exitstatus == 0, "provision failed"

    assert container_running(PROVISION_SERVICE_NAME), "provisioned container not running"
    print("ok: Vaultwarden container running after provision")

    h.delete_deployment(PPO_BIN, REPO_ROOT, PROVISION_DEPLOYMENT_ID)
    run(["docker", "rm", "-f", PROVISION_SERVICE_NAME], check=False)
    run(["docker", "network", "rm", PROVISION_NETWORK], check=False)
    rm_rf_via_docker(PROVISION_DIR)
    print("ok: provisioned deployment/container/network torn down")


def delete_deployment():
    step(f"ddep: delete deployment '{DEPLOYMENT_ID}'")
    h.delete_deployment(PPO_BIN, REPO_ROOT, DEPLOYMENT_ID)

    result = run([PPO_BIN, "lsd"], cwd=REPO_ROOT)
    assert DEPLOYMENT_ID not in result.stdout, "deployment still listed after ddep"

    result = run([PPO_BIN, "pdei"], cwd=REPO_ROOT, check=False)
    assert result.returncode != 0, "pdei should fail once the active deployment is deleted"
    print("ok: deployment gone from lsd, context cleared")


def delete_customer():
    step(f"dc: delete customer '{CUSTOMER}'")
    h.delete_customer(PPO_BIN, REPO_ROOT, CUSTOMER)


def delete_host():
    step(f"dh: delete host '{HOST_ID}'")
    h.delete_host(PPO_BIN, REPO_ROOT, HOST_ID)


def cleanup(snap):
    step("Cleanup")
    try:
        psql(f'DROP DATABASE IF EXISTS "{DB_NAME}"')
    except Exception as e:
        print(f"warning: failed to drop {DB_NAME}: {e}")

    key_path = os.path.join(KEYS_DIR, f"{CUSTOMER}.txt")
    if os.path.exists(key_path):
        os.remove(key_path)

    backups_dir = os.path.join(PATH_FOR_SERVICE, "backups")
    if os.path.isdir(backups_dir):
        run(["rm", "-rf", backups_dir], check=False)

    run(["docker", "rm", "-f", LIFECYCLE_CONTAINER], check=False)
    run(["docker", "rm", "-f", PROVISION_SERVICE_NAME], check=False)
    run(["docker", "network", "rm", PROVISION_NETWORK], check=False)
    rm_rf_via_docker(PROVISION_DIR)

    if snap is not None:
        restore_snapshot(snap)
    print("ok")


def main():
    snap = None
    try:
        preflight()
        snap = snapshot()
        setup_test_db()
        create_host()
        select_host_direct()
        create_customer()
        select_customer()
        create_deployment()
        select_deployment()
        archive = backup_run()
        sabotage()
        backup_restore(archive)
        verify_restored()
        docker_lifecycle_roundtrip()
        service_catalog_roundtrip()
        provision_roundtrip()
        delete_deployment()
        delete_customer()
        delete_host()
        print(f"\n{PASS}: full workflow succeeded")
        return 0
    except Exception:
        print(f"\n{FAIL}: workflow raised an exception")
        traceback.print_exc()
        return 1
    finally:
        cleanup(snap)


if __name__ == "__main__":
    sys.exit(main())
