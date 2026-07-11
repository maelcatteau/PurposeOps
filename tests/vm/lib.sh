#!/usr/bin/env bash
# Shared config/helpers for setup.sh and vmctl.sh — the VirtualBox test host used to
# verify `ppo bootstrap` end-to-end (see PORTING.md, Phase 10). Sourced, not executed.

VM_NAME="ppo-bootstrap-test"
SNAPSHOT_NAME="clean"
SSH_PORT=2280
SSH_USER="ppo"

VM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SSH_KEY="$VM_DIR/id_ed25519"

SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 -i "$SSH_KEY" -p "$SSH_PORT")

vm_ssh() {
    ssh "${SSH_OPTS[@]}" "$SSH_USER@127.0.0.1" "$@"
}

vm_exists() {
    VBoxManage list vms | grep -q "\"$VM_NAME\""
}

vm_state() {
    VBoxManage showvminfo "$VM_NAME" --machinereadable 2>/dev/null | sed -n 's/^VMState="\(.*\)"$/\1/p'
}

wait_for_ssh() {
    local timeout="${1:-180}" waited=0
    until vm_ssh true >/dev/null 2>&1; do
        sleep 3
        waited=$((waited + 3))
        if [ "$waited" -ge "$timeout" ]; then
            echo "timeout waiting for SSH on 127.0.0.1:$SSH_PORT" >&2
            return 1
        fi
    done
}
