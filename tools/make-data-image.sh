#!/bin/bash
# Build the exFAT data disk (Ventoy-style: MBR + exFAT partition 1) holding
# the real ISO collection. Result: /root/remboot-data.img
set -e
ISO_DIR="/mnt/c/Users/remko/OneDrive - Martin Glas/Desktop/iso"
IMG=/root/remboot-data.img
MNT=/mnt/remboot-data

# WSL2 kernel has no exFAT; mkfs comes from exfatprogs, mounting via FUSE.
command -v mkfs.exfat >/dev/null 2>&1 || { echo "installing exfatprogs..."; DEBIAN_FRONTEND=noninteractive apt-get install -y exfatprogs >/dev/null; }
command -v mount.exfat-fuse >/dev/null 2>&1 || { echo "installing exfat-fuse..."; DEBIAN_FRONTEND=noninteractive apt-get install -y exfat-fuse >/dev/null; }

umount "$MNT" 2>/dev/null || true
OLDLOOP=$(losetup -j "$IMG" | cut -d: -f1)
[ -n "$OLDLOOP" ] && losetup -d $OLDLOOP
rm -f "$IMG"

truncate -s 34G "$IMG"
printf 'label: dos\nstart=2048, type=07\n' | sfdisk -q "$IMG"

LOOP=$(losetup --show -fP "$IMG")
echo "loop: $LOOP"
mkfs.exfat -L REMBOOT "${LOOP}p1" >/dev/null
mkdir -p "$MNT"
mount.exfat-fuse "${LOOP}p1" "$MNT"
echo "copying ISOs..."
time cp -v "$ISO_DIR"/*.iso "$MNT"/
sync
df -h "$MNT"
ls -la "$MNT"
umount "$MNT"
losetup -d "$LOOP"
echo "DONE: $IMG"
