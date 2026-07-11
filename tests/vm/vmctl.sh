#!/usr/bin/env bash
# Lifecycle control for the ppo-bootstrap-test VM, called by ../bootstrap_workflow.py
# (VirtualBox orchestration lives here in shell; the Python side stays focused on driving
# `ppo` itself through pexpect).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./lib.sh

cmd="${1:-}"
[ $# -gt 0 ] && shift

case "$cmd" in
    revert)
        if [ "$(vm_state)" != "poweroff" ] && [ "$(vm_state)" != "aborted" ]; then
            VBoxManage controlvm "$VM_NAME" poweroff 2>/dev/null || true
            sleep 2
        fi
        VBoxManage snapshot "$VM_NAME" restore "$SNAPSHOT_NAME"
        ;;
    start)
        VBoxManage startvm "$VM_NAME" --type headless
        wait_for_ssh "${1:-180}"
        ;;
    stop)
        VBoxManage controlvm "$VM_NAME" poweroff 2>/dev/null || true
        ;;
    ssh)
        vm_ssh "$@"
        ;;
    port)
        echo "$SSH_PORT"
        ;;
    key)
        echo "$SSH_KEY"
        ;;
    *)
        echo "usage: vmctl.sh {revert|start [timeout]|stop|ssh CMD|port|key}" >&2
        exit 1
        ;;
esac
