#!/usr/bin/env python3
"""Live integration test for `ppo backup bootstrap-agent`, against a real VirtualBox VM.

Verifies the self-hosted backup agent end to end: a `ppo` binary and a scoped,
single-deployment config pushed to a deployment's own host, a local cron entry
installed there, and a real ntfy notification on failure — see PORTING.md Phase 11.
Reuses tests/vm/ (already built for Phase 10.2) rather than a third harness.

Requires the VM to already exist: run `tests/vm/setup.sh` once first (see
bootstrap_workflow.py's docstring for details — same VM, same one-time setup).

What it does:
  1. Snapshot hosts.yaml/customers.yaml/context.yaml.
  2. Revert the VM to its "clean" snapshot and boot it headless.
  3. ch: register the VM as a scratch host (127.0.0.1, forwarded port — not "localhost",
     to exercise the real SSH ControlMaster path on the push side).
  4. ppo bootstrap <host>: install Docker only (selects just that one capability from the
     MultiSelect, not all four — this test doesn't need Nushell/Caddy/Netdata).
  5. Start a scratch postgres + a dummy "app" container directly over SSH (not through
     `ppo` — these are test fixtures, not something `ppo` provisions).
  6. cc / cdep: register a scratch customer + deployment with real db_credentials
     pointing at the scratch postgres container.
  7. ppo backup bootstrap-agent <deployment_id> --ntfy-url <unique test topic>: pushes
     the agent (binary + scoped config + agent identity + cron.d file).
  8. Verify directly over SSH (not through `ppo`): pushed binary runs (--help), cron.d
     content is correct, scoped YAMLs are present and correct.
  9. Trigger `backup run --cron` directly over SSH (same env the cron.d file would set,
     but via a fresh one-shot SSH connection rather than waiting for real cron) and
     confirm a `.tar.gz` lands.
  10. Break something (stop the DB container), re-trigger, confirm a nonzero exit AND
      poll the real ntfy topic to confirm the failure notification actually arrived.
  11. Re-run bootstrap-agent, confirm the cron.d file is replaced, not duplicated.
  12. ddep / dc / dh: delete the scratch deployment/customer/host.

Cleanup (VM shutdown + config restore) always runs, even on failure. Uses scratch data
only — never touches a real customer's secrets.

Usage: python3 tests/backup_agent_workflow.py
Env: PPO_BIN to override the binary path (default: target/debug/ppo).
"""

import os
import sys
import time
import traceback
import uuid

import pexpect

import ppo_test_helpers as h

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PPO_BIN = os.environ.get("PPO_BIN", os.path.join(REPO_ROOT, "target", "debug", "ppo"))
CONFIG_DIR = os.path.join(REPO_ROOT, "PurposeOps-config")

VM_DIR = os.path.join(REPO_ROOT, "tests", "vm")
VMCTL = os.path.join(VM_DIR, "vmctl.sh")
VM_KEY = os.path.join(VM_DIR, "id_ed25519")
VM_NAME = "ppo-bootstrap-test"

HOST_ID = "backup-agent-test-vm"
CUSTOMER = "BackupAgentTest"
ABBREV = "bat"
DEPLOYMENT_ID = "backup-agent-test-dep"
# Backups now live under the deployment's own path_for_service (see PORTING.md), not a
# centralized ~/backups/<abbrev>/<host_id> tree — this must match what create_deployment()
# actually sends for "Path for service on host".
PATH_FOR_SERVICE = "/home/ppo"

DB_CONTAINER = "agent-test-db"
APP_CONTAINER = "agent-test-app"
DB_NAME = "agent_test_db"
DB_USER = "agent_test"
DB_PASSWORD = "agent-test-pw"
DB_PORT = "5432"

NTFY_TOPIC = f"ppo-test-{uuid.uuid4().hex[:12]}"
NTFY_URL = f"https://ntfy.sh/{NTFY_TOPIC}"

PASS = "\033[32mPASS\033[0m"
FAIL = "\033[31mFAIL\033[0m"

RIGHT_ARROW = "\x1b[C"


def step(msg):
    print(f"\n== {msg}")


def run(cmd, check=True, **kw):
    return h.run(cmd, check=check, **kw)


def vmctl(*args, timeout=None, check=True):
    return h.vmctl(VMCTL, *args, timeout=timeout, check=check)


def vm_ssh(cmd, check=True, timeout=60):
    return h.vm_ssh(VMCTL, cmd, check=check, timeout=timeout)


def spawn(args, timeout=15):
    return h.spawn(PPO_BIN, REPO_ROOT, args, timeout=timeout)


def preflight():
    step("Preflight checks")
    if not os.path.isfile(PPO_BIN):
        raise RuntimeError(f"ppo binary not found at {PPO_BIN} — run `cargo build` first")
    if not os.path.isfile(VM_KEY):
        raise RuntimeError(f"{VM_KEY} not found — run tests/vm/setup.sh first")

    vms = run(["VBoxManage", "list", "vms"]).stdout
    if f'"{VM_NAME}"' not in vms:
        raise RuntimeError(f"VM '{VM_NAME}' does not exist — run tests/vm/setup.sh first")

    hosts = h.read_file(os.path.join(CONFIG_DIR, "hosts.yaml"))
    customers = h.read_file(os.path.join(CONFIG_DIR, "customers.yaml"))
    if f"{HOST_ID}:" in hosts or f"{CUSTOMER}:" in customers:
        raise RuntimeError(
            f"'{HOST_ID}' or '{CUSTOMER}' already present in config — leftover from a "
            "previous failed run? Check and clean up manually."
        )
    print("ok")


def snapshot():
    return {
        "hosts.yaml": h.read_file(os.path.join(CONFIG_DIR, "hosts.yaml")),
        "customers.yaml": h.read_file(os.path.join(CONFIG_DIR, "customers.yaml")),
        "context.yaml": h.read_file(os.path.join(CONFIG_DIR, "context.yaml")),
    }


def restore_snapshot(snap):
    for name, content in snap.items():
        h.write_file(os.path.join(CONFIG_DIR, name), content)


def start_vm():
    step("Reverting VM to 'clean' snapshot and booting it")
    vmctl("revert", timeout=60)
    vmctl("start", "240", timeout=260)
    print("ok, SSH is up")


def create_host():
    port = vmctl("port").stdout.strip()
    step(f"ch: register scratch host '{HOST_ID}' -> 127.0.0.1:{port}")
    h.create_host(
        PPO_BIN, REPO_ROOT,
        host_id=HOST_ID, hostname="127.0.0.1", user="ppo", port=port, identity_file=VM_KEY,
    )


def bootstrap_docker_only():
    step(f"ppo bootstrap {HOST_ID} (Docker only)")
    child = spawn(f"bootstrap {HOST_ID}", timeout=90)
    for label in ("Docker", "Nushell", "Caddy", "Netdata"):
        child.expect(f"⬜ {label}".encode())
    child.expect("faut-il installer")
    child.send(" ")  # toggle the first/highlighted item (Docker) only
    child.sendline("")  # Enter confirms the selection
    child.expect("Installer .*\\?")
    child.sendline("y")
    child.expect(pexpect.EOF, timeout=180)
    child.close()
    assert child.exitstatus == 0, "ppo bootstrap failed"


def start_test_containers():
    step("Starting scratch postgres + app containers directly over SSH")
    vm_ssh(f"sudo docker rm -f {DB_CONTAINER} {APP_CONTAINER}", check=False)
    vm_ssh(
        f"sudo docker run -d --name {DB_CONTAINER} "
        f"-e POSTGRES_USER={DB_USER} -e POSTGRES_PASSWORD={DB_PASSWORD} -e POSTGRES_DB={DB_NAME} "
        f"postgres:16-alpine"
    )
    vm_ssh(f"sudo docker run -d --name {APP_CONTAINER} alpine:3.20 sleep infinity")

    for _ in range(20):
        result = vm_ssh(f"sudo docker exec {DB_CONTAINER} pg_isready -U {DB_USER}", check=False)
        if result.returncode == 0:
            print("ok, postgres is ready")
            return
        time.sleep(2)
    raise RuntimeError("postgres never became ready")


def create_customer():
    step(f"cc: create scratch customer '{CUSTOMER}'")
    h.create_customer(
        PPO_BIN, REPO_ROOT,
        name=CUSTOMER, abbrev=ABBREV, host_id=HOST_ID, path_on_host="/home/ppo",
    )


def select_customer():
    run([PPO_BIN, "sc", CUSTOMER], cwd=REPO_ROOT)


def create_deployment():
    step(f"cdep: create deployment '{DEPLOYMENT_ID}'")
    child = spawn("cdep")
    child.expect("Service name")
    child.sendline("Backup Agent Test Service")
    child.expect("Host ID")
    child.sendline(HOST_ID)
    child.expect("Path for service on host")
    child.sendline(PATH_FOR_SERVICE)
    child.expect("Path for docker-compose file")
    child.sendline("/home/ppo/docker-compose.yml")
    child.expect("Deployment id")
    child.sendline(DEPLOYMENT_ID)
    child.expect("base de donn")
    child.sendline("y")
    child.expect("Container name")
    child.sendline(APP_CONTAINER)
    child.expect("DB container name")
    child.sendline(DB_CONTAINER)
    child.expect("Database name")
    child.sendline(DB_NAME)
    child.expect("DB credentials - host")
    child.sendline(DB_CONTAINER)
    child.expect("DB credentials - port")
    child.sendline(DB_PORT)
    child.expect("DB credentials - user")
    child.sendline(DB_USER)
    child.expect("DB credentials - password")
    child.sendline(DB_PASSWORD)
    child.expect("ce d.*ploiement")
    child.sendline("y")
    child.expect(pexpect.EOF, timeout=15)
    child.close()
    assert child.exitstatus == 0, "cdep failed"


def bootstrap_agent():
    step(f"ppo backup bootstrap-agent {DEPLOYMENT_ID} --ntfy-url {NTFY_URL}")
    result = run(
        [PPO_BIN, "backup", "bootstrap-agent", DEPLOYMENT_ID, "--ntfy-url", NTFY_URL, "--keep-last", "3"],
        cwd=REPO_ROOT,
        timeout=180,
    )
    print(result.stdout)
    assert "Agent de backup installé" in result.stdout, "bootstrap-agent did not report success"


def remote_bin_path():
    return "/home/ppo/dev/nu-modules/PurposeOps/target/release/ppo"


def verify_agent_pushed():
    step("Verifying the pushed agent directly over SSH (not through ppo)")

    out = vm_ssh(f"{remote_bin_path()} --help").stdout
    assert "PurposeOps" in out or "Usage" in out, f"remote binary --help unexpected: {out!r}"

    cron_content = vm_ssh(f"cat /etc/cron.d/ppo-backup-{DEPLOYMENT_ID}").stdout
    assert f"NTFY_URL={NTFY_URL}" in cron_content, f"cron.d missing NTFY_URL: {cron_content!r}"
    assert "backup run --cron --keep-last 3" in cron_content, f"cron.d missing expected command: {cron_content!r}"
    assert DEPLOYMENT_ID in cron_content

    scoped_hosts = vm_ssh(
        "cat /home/ppo/dev/nu-modules/PurposeOps/PurposeOps-config/hosts.yaml"
    ).stdout
    assert "hostname: localhost" in scoped_hosts, f"scoped hosts.yaml wrong: {scoped_hosts!r}"

    scoped_customers = vm_ssh(
        "cat /home/ppo/dev/nu-modules/PurposeOps/PurposeOps-config/customers.yaml"
    ).stdout
    assert DEPLOYMENT_ID in scoped_customers
    assert "enc:" in scoped_customers, "scoped customers.yaml password not encrypted"

    agent_key = vm_ssh(
        f"stat -c '%a' /home/ppo/.config/ppo/keys/agent-{DEPLOYMENT_ID}.txt"
    ).stdout.strip()
    assert agent_key == "600", f"agent identity file has wrong permissions: {agent_key!r}"

    print("ok: binary, cron.d, scoped config, and agent identity all correct")


def trigger_backup_run(expect_success):
    # A fresh one-shot SSH connection (not ppo's own persistent ControlMaster), same as
    # real cron would use — matters because group membership (the docker-group fix, see
    # PORTING.md 11.4) only takes effect for a new login session.
    cmd = f"NTFY_URL={NTFY_URL} {remote_bin_path()} backup run --cron --keep-last 3"
    result = vm_ssh(cmd, check=False, timeout=60)
    print(result.stdout)
    print(result.stderr)
    if expect_success:
        assert result.returncode == 0, f"backup run --cron failed unexpectedly: {result.stderr}"
    else:
        assert result.returncode != 0, "backup run --cron succeeded but was expected to fail"
    return result


def verify_backup_archive_exists():
    step("Triggering backup run --cron over SSH and checking for an archive")
    trigger_backup_run(expect_success=True)
    out = vm_ssh(f"ls -1 {PATH_FOR_SERVICE}/backups/*.tar.gz").stdout
    assert out.strip(), "no backup archive found after backup run --cron"
    print(f"ok: archive present ({out.strip().splitlines()[-1]})")


def poll_ntfy(expect_substring, timeout=30):
    deadline = time.time() + timeout
    poll_url = f"{NTFY_URL}/json?poll=1"
    while time.time() < deadline:
        result = run(["curl", "-fsS", poll_url], check=False)
        if expect_substring in result.stdout:
            return True
        time.sleep(3)
    return False


def verify_failure_notifies():
    step("Breaking the DB container and confirming a real ntfy notification arrives")
    vm_ssh(f"sudo docker stop {DB_CONTAINER}")
    trigger_backup_run(expect_success=False)
    vm_ssh(f"sudo docker start {DB_CONTAINER}")
    for _ in range(15):
        result = vm_ssh(f"sudo docker exec {DB_CONTAINER} pg_isready -U {DB_USER}", check=False)
        if result.returncode == 0:
            break
        time.sleep(2)

    assert poll_ntfy("ppo backup run"), "ntfy notification never arrived on real polling"
    print("ok: nonzero exit and a real ntfy notification both confirmed")


def verify_idempotent_reinstall():
    step("Re-running bootstrap-agent to confirm the cron.d file is replaced, not duplicated")
    bootstrap_agent()
    cron_content = vm_ssh(f"cat /etc/cron.d/ppo-backup-{DEPLOYMENT_ID}").stdout
    assert cron_content.count(DEPLOYMENT_ID) <= 2, f"cron.d looks duplicated: {cron_content!r}"
    print("ok: cron.d replaced cleanly")


def delete_deployment():
    step(f"ddep: delete deployment '{DEPLOYMENT_ID}'")
    h.delete_deployment(PPO_BIN, REPO_ROOT, DEPLOYMENT_ID)


def delete_customer():
    step(f"dc: delete customer '{CUSTOMER}'")
    h.delete_customer(PPO_BIN, REPO_ROOT, CUSTOMER)


def delete_host():
    step(f"dh: delete host '{HOST_ID}'")
    h.delete_host(PPO_BIN, REPO_ROOT, HOST_ID)


def cleanup(snap):
    step("Cleanup")
    vmctl("stop", timeout=60, check=False)
    if snap is not None:
        restore_snapshot(snap)
    key_path = os.path.expanduser(f"~/.config/ppo/keys/agent-{DEPLOYMENT_ID}.txt")
    if os.path.exists(key_path):
        os.remove(key_path)
    print("ok")


def main():
    snap = None
    try:
        preflight()
        snap = snapshot()
        start_vm()
        create_host()
        bootstrap_docker_only()
        start_test_containers()
        create_customer()
        select_customer()
        create_deployment()
        bootstrap_agent()
        verify_agent_pushed()
        verify_backup_archive_exists()
        verify_failure_notifies()
        verify_idempotent_reinstall()
        delete_deployment()
        delete_customer()
        delete_host()
        print(f"\n{PASS}: backup agent workflow succeeded")
        return 0
    except Exception:
        print(f"\n{FAIL}: workflow raised an exception")
        traceback.print_exc()
        return 1
    finally:
        cleanup(snap)


if __name__ == "__main__":
    sys.exit(main())
