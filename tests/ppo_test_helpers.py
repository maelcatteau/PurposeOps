"""Shared helpers for the live `tests/*.py` scripts.

Not a `cargo test` target itself — imported by `integration_workflow.py`,
`bootstrap_workflow.py`, and `backup_agent_workflow.py`. Factored out because the
`ch`/`dh`/`cc` wizard prompt sequences and `vmctl`/`vm_ssh` VM boilerplate were
duplicated verbatim (or near-verbatim) across those three scripts — a real
double-maintenance risk, not premature abstraction: this session already had to fix
the same `CONFIG_DIR` path bug independently in two of them before this refactor.

Each script keeps its own `PPO_BIN`/`REPO_ROOT`/scratch-data constants and its own
`main()`/cleanup flow — only the mechanical "drive this wizard" and "run this VM
command" pieces move here.
"""

import subprocess
import sys

import pexpect


def run(cmd, check=True, **kw):
    result = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if check and result.returncode != 0:
        raise RuntimeError(
            f"command failed ({result.returncode}): {' '.join(cmd)}\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )
    return result


def spawn(ppo_bin, repo_root, args, timeout=15):
    child = pexpect.spawn(f"{ppo_bin} {args}", cwd=repo_root, timeout=timeout)
    child.logfile = sys.stdout.buffer
    return child


def read_file(path):
    with open(path) as f:
        return f.read()


def write_file(path, content):
    with open(path, "w") as f:
        f.write(content)


def create_host(ppo_bin, repo_root, *, host_id, hostname, user, port, identity_file, arch="x86_64"):
    """Drives the `ch` wizard. `identity_file` may be `""` (localhost, no key needed)."""
    child = spawn(ppo_bin, repo_root, "ch")
    child.expect("host_name")
    child.sendline(host_id)
    child.expect("hostname")
    child.sendline(hostname)
    child.expect("user")
    child.sendline(user)
    child.expect("port")
    child.sendline(port)
    child.expect("ssh id file")
    child.sendline(identity_file)
    child.expect("architecture")
    child.sendline(arch)
    child.expect("Valider")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "ch failed"


def delete_host(ppo_bin, repo_root, host_id):
    child = spawn(ppo_bin, repo_root, "dh")
    child.expect("Select host")
    child.send(host_id)
    child.sendline("")
    child.expect("Delete")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "dh failed"


def create_customer(ppo_bin, repo_root, *, name, abbrev, host_id, path_on_host):
    """Drives the `cc` wizard."""
    child = spawn(ppo_bin, repo_root, "cc")
    child.expect("Customer name")
    child.sendline(name)
    child.expect("Abbreviation")
    child.sendline(abbrev)
    child.expect("Host ID")
    child.sendline(host_id)
    child.expect("Path on host")
    child.sendline(path_on_host)
    child.expect("Create")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "cc failed"


def delete_customer(ppo_bin, repo_root, name):
    child = spawn(ppo_bin, repo_root, "dc")
    child.expect("Select customer")
    child.send(name)
    child.sendline("")
    child.expect("Delete")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "dc failed"


def delete_deployment(ppo_bin, repo_root, deployment_id):
    child = spawn(ppo_bin, repo_root, f"ddep {deployment_id}")
    child.expect("Delete")
    child.sendline("y")
    child.expect(pexpect.EOF)
    child.close()
    assert child.exitstatus == 0, "ddep failed"


def vmctl(vmctl_path, *args, timeout=None, check=True):
    return run([vmctl_path, *args], check=check, timeout=timeout)


def vm_ssh(vmctl_path, cmd, check=True, timeout=60):
    return vmctl(vmctl_path, "ssh", cmd, timeout=timeout, check=check)
