#!/usr/bin/env python3
"""Live integration test for `ppo bootstrap`, against a real VirtualBox VM.

Unlike integration_workflow.py (which targets the local demo-odoo containers),
`bootstrap.rs` installs system-level software over SSH — Docker, Nushell, Caddy,
Netdata — so a container test host would be misleading here: Docker-in-Docker and
systemd-in-a-container are both known-fragile, and the real fleet is full VPS boxes
anyway. This drives a VirtualBox VM instead (see tests/vm/), reverted to a clean
snapshot before every run so the test is repeatable without re-provisioning.

Requires the VM to already exist: run `tests/vm/setup.sh` once first (downloads an
Ubuntu 24.04 cloud image, provisions it via cloud-init, snapshots it "clean" — slow,
one-time, not part of this script).

What it does:
  1. Snapshot hosts.yaml/context.yaml.
  2. Revert the VM to its "clean" snapshot and boot it headless.
  3. ch: register the VM as a scratch host (hostname 127.0.0.1, forwarded port —
     deliberately NOT "localhost", so this exercises the real SSH ControlMaster path
     that `bootstrap.rs`/`provision.rs` use against actual remote hosts).
  4. ppo bootstrap: detect (expect all 4 missing on a fresh VM) → select all via the
     MultiSelect's "select all" hotkey (Right arrow) → confirm → install.
  5. Verify each capability directly over SSH (not through `ppo`): docker actually
     runs a container, nu/caddy report a version, netdata answers its local API.
  6. Re-run `ppo bootstrap` and verify it reports everything already installed
     (idempotency — the whole point of live-checking instead of caching state).
  7. dh: delete the scratch host.

Cleanup (VM shutdown + config restore) always runs, even on failure.

Usage: python3 tests/bootstrap_workflow.py
Env: PPO_BIN to override the binary path (default: target/debug/ppo).
"""

import os
import subprocess
import sys
import time
import traceback

import pexpect

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PPO_BIN = os.environ.get("PPO_BIN", os.path.join(REPO_ROOT, "target", "debug", "ppo"))
CONFIG_DIR = os.path.join(REPO_ROOT, "PurposeOps-config")

VM_DIR = os.path.join(REPO_ROOT, "tests", "vm")
VMCTL = os.path.join(VM_DIR, "vmctl.sh")
VM_KEY = os.path.join(VM_DIR, "id_ed25519")
VM_NAME = "ppo-bootstrap-test"

HOST_ID = "bootstrap-test-vm"

PASS = "\033[32mPASS\033[0m"
FAIL = "\033[31mFAIL\033[0m"

RIGHT_ARROW = "\x1b[C"


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


def vmctl(*args, timeout=None, check=True):
    return run([VMCTL, *args], check=check, timeout=timeout)


def vm_ssh(cmd, check=True):
    return vmctl("ssh", cmd, timeout=60, check=check)


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
    if not os.path.isfile(VM_KEY):
        raise RuntimeError(f"{VM_KEY} not found — run tests/vm/setup.sh first")

    vms = run(["VBoxManage", "list", "vms"]).stdout
    if f'"{VM_NAME}"' not in vms:
        raise RuntimeError(f"VM '{VM_NAME}' does not exist — run tests/vm/setup.sh first")

    hosts = read_file(os.path.join(CONFIG_DIR, "hosts.yaml"))
    if f"{HOST_ID}:" in hosts:
        raise RuntimeError(
            f"'{HOST_ID}' already present in hosts.yaml — leftover from a previous "
            "failed run? Check and clean up manually."
        )
    print("ok")


def snapshot():
    return {
        "hosts.yaml": read_file(os.path.join(CONFIG_DIR, "hosts.yaml")),
        "context.yaml": read_file(os.path.join(CONFIG_DIR, "context.yaml")),
    }


def restore_snapshot(snap):
    for name, content in snap.items():
        write_file(os.path.join(CONFIG_DIR, name), content)


def start_vm():
    step("Reverting VM to 'clean' snapshot and booting it")
    vmctl("revert", timeout=60)
    vmctl("start", "240", timeout=260)
    print("ok, SSH is up")


def create_host():
    step(f"ch: register scratch host '{HOST_ID}' -> 127.0.0.1:{vmctl('port').stdout.strip()}")
    port = vmctl("port").stdout.strip()
    child = spawn("ch")
    child.expect("host_name")
    child.sendline(HOST_ID)
    child.expect("hostname")
    child.sendline("127.0.0.1")
    child.expect("user")
    child.sendline("ppo")
    child.expect("port")
    child.sendline(port)
    child.expect("ssh id file")
    child.sendline(VM_KEY)
    child.expect("architecture")
    child.sendline("x86_64")
    child.expect("Valider")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "ch failed"


def run_bootstrap_fresh():
    step(f"ppo bootstrap {HOST_ID} (fresh VM — expect all 4 missing)")
    child = spawn(f"bootstrap {HOST_ID}", timeout=90)
    for label in ("Docker", "Nushell", "Caddy", "Netdata"):
        child.expect(f"⬜ {label}".encode())  # pexpect patterns must be ASCII unless bytes
    child.expect("faut-il installer")
    child.send(RIGHT_ARROW)  # inquire MultiSelect: Right arrow = select all
    child.sendline("")  # Enter, confirms the selection
    child.expect("Installer .*\\?")
    child.sendline("y")
    child.expect(pexpect.EOF, timeout=600)  # apt/curl installs over SSH, can take a while
    child.close()
    assert child.exitstatus == 0, "ppo bootstrap failed"


def verify_installed():
    step("Verifying each capability directly over SSH (not through ppo)")

    out = vm_ssh("docker --version").stdout
    assert "Docker version" in out, f"docker --version unexpected: {out!r}"

    out = vm_ssh("sudo docker run --rm hello-world").stdout
    assert "Hello from Docker" in out, f"docker run hello-world failed: {out!r}"

    out = vm_ssh("nu --version").stdout
    assert out.strip(), "nu --version produced no output"

    out = vm_ssh("caddy version").stdout
    assert out.strip(), "caddy version produced no output"

    # Netdata can take a little while after install to finish its startup/initial
    # collection cycle — its API answers 503 until then, not a bootstrap.rs bug.
    out = ""
    for _ in range(12):
        result = vm_ssh("curl -fsS http://localhost:19999/api/v1/info", check=False)
        if result.returncode == 0:
            out = result.stdout
            break
        time.sleep(5)
    assert "version" in out, f"netdata local API unreachable after retries: {out!r}"

    print("ok: docker (+ a real container run), nu, caddy, netdata all functional")


def run_bootstrap_idempotent():
    step(f"ppo bootstrap {HOST_ID} again (expect nothing missing)")
    child = spawn(f"bootstrap {HOST_ID}", timeout=60)
    for label in ("Docker", "Nushell", "Caddy", "Netdata"):
        child.expect(f"✅ {label}".encode())
    child.expect("d.*j.* install")  # "Tout est déjà installé." — avoid the accented bytes
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "ppo bootstrap (idempotent run) failed"
    print("ok: no re-installation attempted")


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
    vmctl("stop", timeout=60, check=False)
    if snap is not None:
        restore_snapshot(snap)
    print("ok")


def main():
    snap = None
    try:
        preflight()
        snap = snapshot()
        start_vm()
        create_host()
        run_bootstrap_fresh()
        verify_installed()
        run_bootstrap_idempotent()
        delete_host()
        print(f"\n{PASS}: bootstrap workflow succeeded")
        return 0
    except Exception:
        print(f"\n{FAIL}: workflow raised an exception")
        traceback.print_exc()
        return 1
    finally:
        cleanup(snap)


if __name__ == "__main__":
    sys.exit(main())
