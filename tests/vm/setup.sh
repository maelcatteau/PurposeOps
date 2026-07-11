#!/usr/bin/env bash
# One-time (idempotent) provisioning of the ppo-bootstrap-test VirtualBox VM used by
# ../bootstrap_workflow.py. Downloads an Ubuntu 24.04 LTS cloud image (matches the distros
# actually in the fleet — see PORTING.md Phase 10), provisions it via cloud-init with a
# passwordless-sudo user and an injected SSH key (what a freshly handed-over VPS looks
# like), then snapshots it "clean" so the test can revert to a known-good state on every
# run instead of re-provisioning from scratch each time.
#
# Safe to re-run: exits immediately if the VM already exists. Pass --recreate to tear it
# down and rebuild from scratch (e.g. after changing MEM_MB/DISK_MB below).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./lib.sh

RECREATE=0
[ "${1:-}" = "--recreate" ] && RECREATE=1

IMG_URL="https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img"
IMG_RAW="$VM_DIR/ubuntu-24.04-cloudimg.img"
VDI="$VM_DIR/ubuntu-24.04.vdi"
SEED_ISO="$VM_DIR/seed.iso"
DISK_MB=12288
MEM_MB=2048
CPUS=2

if [ "$RECREATE" = 1 ] && vm_exists; then
    echo "--recreate: tearing down existing '$VM_NAME'..."
    VBoxManage controlvm "$VM_NAME" poweroff 2>/dev/null || true
    sleep 2
    VBoxManage unregistervm "$VM_NAME" --delete
fi

if vm_exists; then
    echo "VM '$VM_NAME' already exists — skipping (pass --recreate to rebuild)."
    exit 0
fi

if [ ! -f "$SSH_KEY" ]; then
    echo "Generating SSH keypair for the test VM..."
    ssh-keygen -t ed25519 -N "" -f "$SSH_KEY" -C ppo-bootstrap-test -q
fi

if [ ! -f "$IMG_RAW" ]; then
    echo "Downloading Ubuntu 24.04 cloud image..."
    curl -fL --progress-bar -o "$IMG_RAW" "$IMG_URL"
fi

echo "Converting image to VDI and resizing to ${DISK_MB}MB..."
rm -f "$VDI"
qemu-img convert -O vdi "$IMG_RAW" "$VDI"
VBoxManage modifymedium disk "$VDI" --resize "$DISK_MB"

echo "Building cloud-init seed ISO..."
PUBKEY=$(cat "$SSH_KEY.pub")
cat >"$VM_DIR/user-data" <<EOF
#cloud-config
hostname: $VM_NAME
users:
  - name: $SSH_USER
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    ssh_authorized_keys:
      - $PUBKEY
ssh_pwauth: false
package_update: false
runcmd:
  # Ubuntu 24.04's cloud image enables both the socket-activated ssh.socket and the
  # plain ssh.service; both try to bind :22 and ssh.service loses the race on every
  # boot after the first. Disable the socket unit so ssh.service owns the port outright
  # — runs once per instance, so this sticks in the "clean" snapshot for every later boot.
  - systemctl disable --now ssh.socket || true
  - systemctl enable --now ssh.service
EOF
cat >"$VM_DIR/meta-data" <<EOF
instance-id: $VM_NAME
local-hostname: $VM_NAME
EOF
genisoimage -output "$SEED_ISO" -volid cidata -joliet -rock "$VM_DIR/user-data" "$VM_DIR/meta-data" >/dev/null

echo "Creating VM '$VM_NAME'..."
VBoxManage createvm --name "$VM_NAME" --ostype Ubuntu_64 --register
VBoxManage modifyvm "$VM_NAME" \
    --memory "$MEM_MB" --cpus "$CPUS" --ioapic on --hwvirtex on \
    --nic1 nat --natpf1 "ssh,tcp,127.0.0.1,$SSH_PORT,,22" \
    --uart1 0x3F8 4 --uartmode1 file "$VM_DIR/serial.log" \
    --boot1 disk --boot2 none --boot3 none --boot4 none
VBoxManage storagectl "$VM_NAME" --name SATA --add sata --controller IntelAhci
VBoxManage storageattach "$VM_NAME" --storagectl SATA --port 0 --device 0 --type hdd --medium "$VDI"
VBoxManage storageattach "$VM_NAME" --storagectl SATA --port 1 --device 0 --type dvddrive --medium "$SEED_ISO"

echo "Booting for first-time cloud-init provisioning..."
VBoxManage startvm "$VM_NAME" --type headless
wait_for_ssh 240
vm_ssh "sudo cloud-init status --wait"

echo "Provisioning complete — shutting down cleanly before snapshotting..."
# A hard `controlvm poweroff` right here risks freezing the disk before newly-written
# files (freshly regenerated SSH host keys, in particular) are flushed — sshd then fails
# its config test on every later boot from this snapshot even though it worked on this
# first one. Ask the guest to shut down itself and wait for it, so the filesystem is
# cleanly synced/unmounted first.
vm_ssh "sudo sync; sudo shutdown -h now" || true
waited=0
while [ "$(vm_state)" != "poweroff" ] && [ "$(vm_state)" != "aborted" ]; do
    sleep 3
    waited=$((waited + 3))
    if [ "$waited" -ge 120 ]; then
        echo "Graceful shutdown timed out — forcing poweroff" >&2
        VBoxManage controlvm "$VM_NAME" poweroff
        break
    fi
done
sleep 2
echo "Taking snapshot '$SNAPSHOT_NAME'..."
VBoxManage snapshot "$VM_NAME" take "$SNAPSHOT_NAME"

echo "Done. Run ../bootstrap_workflow.py to exercise 'ppo bootstrap' against it."
