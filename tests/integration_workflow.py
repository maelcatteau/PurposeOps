#!/usr/bin/env python3
"""Live integration test for ppo: full CRUD + backup/restore lifecycle.

Not a `cargo test` target — `cc`/`ch`/`cdep` are interactive `inquire` wizards
that need a real PTY, so this drives the real `ppo` binary through pexpect,
matching how every feature in this project has been verified by hand. See
PORTING.md for the project's "verify live, no automated test suite" convention.

What it does, against real PurposeOps-config and the local `odoo-demo`/
`odoo-demo-db` Docker containers (must already be running):

  1. Snapshot hosts.yaml/customers.yaml/context.yaml.
  2. Create a scratch Postgres DB (`inttest_db`) with one marker row.
  3. ch: create a scratch host targeting hostname=localhost.
  4. cc: create a scratch customer on that host.
  5. cdep: create a deployment pointing at odoo-demo/odoo-demo-db/inttest_db.
  6. backup run: back up inttest_db.
  7. Drop the marker table (simulate data loss).
  8. backup restore --force: restore from the archive just created.
  9. Verify the marker row is back with the correct value.
  10. ddep: delete the deployment directly (not via customer cascade).
  11. dc / dh: delete the now deployment-less scratch customer and host.
  12. Restore hosts.yaml/customers.yaml/context.yaml byte-for-byte, drop
      inttest_db, remove the scratch customer's generated age key, remove the
      backup archive(s) created on disk.

Cleanup (step 12) always runs, even on failure, so a crashed run doesn't leave
scratch state behind. Exit code 0 = pass, 1 = fail.

Usage: python3 tests/integration_workflow.py
Env: PPO_BIN to override the binary path (default: target/debug/ppo).
"""

import os
import subprocess
import sys
import traceback

import pexpect

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

PASS = "\033[32mPASS\033[0m"
FAIL = "\033[31mFAIL\033[0m"


def step(msg):
    print(f"\n== {msg}")


def run(cmd, check=True, **kw):
    result = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if check and result.returncode != 0:
        raise RuntimeError(
            f"command failed ({result.returncode}): {' '.join(cmd)}\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )
    return result


def psql(sql, database="postgres", check=True):
    return run(
        [
            "docker", "exec", "-e", "PGPASSWORD=odoo_demo", "odoo-demo-db",
            "psql", "-h", "localhost", "-U", "odoo", "-d", database, "-tAc", sql,
        ],
        check=check,
    )


def spawn(args, timeout=15):
    child = pexpect.spawn(f"{PPO_BIN} {args}", cwd=REPO_ROOT, timeout=timeout)
    child.logfile = sys.stdout.buffer
    return child


def read_file(path):
    with open(path) as f:
        return f.read()


def write_file(path, content):
    with open(path, "w") as f:
        f.write(content)


def preflight():
    step("Preflight checks")
    if not os.path.isfile(PPO_BIN):
        raise RuntimeError(f"ppo binary not found at {PPO_BIN} — run `cargo build` first")

    names = run(["docker", "ps", "--format", "{{.Names}}"]).stdout.split()
    for c in ("odoo-demo", "odoo-demo-db"):
        if c not in names:
            raise RuntimeError(f"container '{c}' is not running — start demo-odoo first")

    customers = read_file(os.path.join(CONFIG_DIR, "customers.yaml"))
    hosts = read_file(os.path.join(CONFIG_DIR, "hosts.yaml"))
    if f"{CUSTOMER}:" in customers or f"{HOST_ID}:" in hosts:
        raise RuntimeError(
            f"'{CUSTOMER}' or '{HOST_ID}' already present in config — "
            "leftover from a previous failed run? Check and clean up manually."
        )
    print("ok")


def snapshot():
    return {
        "hosts.yaml": read_file(os.path.join(CONFIG_DIR, "hosts.yaml")),
        "customers.yaml": read_file(os.path.join(CONFIG_DIR, "customers.yaml")),
        "context.yaml": read_file(os.path.join(CONFIG_DIR, "context.yaml")),
    }


def restore_snapshot(snap):
    for name, content in snap.items():
        write_file(os.path.join(CONFIG_DIR, name), content)


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
    child = spawn("ch")
    child.expect("host_name")
    child.sendline(HOST_ID)
    child.expect("hostname")
    child.sendline("localhost")
    child.expect("user")
    child.sendline("ngner")
    child.expect("port")
    child.sendline("22")
    child.expect("ssh id file")
    child.sendline("")
    child.expect("architecture")
    child.sendline("x86_64")
    child.expect("Valider")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "ch failed"


def create_customer():
    step(f"cc: create scratch customer '{CUSTOMER}'")
    child = spawn("cc")
    child.expect("Customer name")
    child.sendline(CUSTOMER)
    child.expect("Abbreviation")
    child.sendline(ABBREV)
    child.expect("Host ID")
    child.sendline(HOST_ID)
    child.expect("Path on host")
    child.sendline("/home/ngner/dev/demo-odoo")
    child.expect("Create")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "cc failed"


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
    child.sendline("/home/ngner/dev/demo-odoo")
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

    customers = read_file(os.path.join(CONFIG_DIR, "customers.yaml"))
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


def delete_deployment():
    step(f"ddep: delete deployment '{DEPLOYMENT_ID}'")
    child = spawn(f"ddep {DEPLOYMENT_ID}")
    child.expect("Delete")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "ddep failed"

    result = run([PPO_BIN, "lsd"], cwd=REPO_ROOT)
    assert DEPLOYMENT_ID not in result.stdout, "deployment still listed after ddep"

    result = run([PPO_BIN, "pdei"], cwd=REPO_ROOT, check=False)
    assert result.returncode != 0, "pdei should fail once the active deployment is deleted"
    print("ok: deployment gone from lsd, context cleared")


def delete_customer():
    step(f"dc: delete customer '{CUSTOMER}'")
    child = spawn("dc")
    child.expect("Select customer")
    child.send(CUSTOMER)
    child.sendline("")
    child.expect("Delete")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "dc failed"


def delete_host():
    step(f"dh: delete host '{HOST_ID}'")
    child = spawn("dh")
    child.expect("Select host")
    child.send(HOST_ID)
    child.sendline("")
    child.expect("Delete")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "dh failed"


def cleanup(snap):
    step("Cleanup")
    try:
        psql(f'DROP DATABASE IF EXISTS "{DB_NAME}"')
    except Exception as e:
        print(f"warning: failed to drop {DB_NAME}: {e}")

    key_path = os.path.join(KEYS_DIR, f"{CUSTOMER}.txt")
    if os.path.exists(key_path):
        os.remove(key_path)

    backups_dir = os.path.expanduser(f"~/backups/{ABBREV}")
    if os.path.isdir(backups_dir):
        run(["rm", "-rf", backups_dir], check=False)

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
        create_customer()
        select_customer()
        create_deployment()
        select_deployment()
        archive = backup_run()
        sabotage()
        backup_restore(archive)
        verify_restored()
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
