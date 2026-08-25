#!/bin/bash
# Copy remboot.conf.example into the exFAT data image as /remboot.conf, so the
# app reads it at boot. Re-run after editing the config.
set -e
IMG=/root/remboot-data.img
MNT=/mnt/remboot-data
SRC=/mnt/c/GitHub/RemBoot/remboot.conf.example

command -v mount.exfat-fuse >/dev/null 2>&1 || { DEBIAN_FRONTEND=noninteractive apt-get install -y exfat-fuse >/dev/null; }
umount "$MNT" 2>/dev/null || true
OLD=$(losetup -j "$IMG" | cut -d: -f1)
[ -n "$OLD" ] && losetup -d $OLD
LOOP=$(losetup --show -fP "$IMG")
mkdir -p "$MNT"
mount.exfat-fuse "${LOOP}p1" "$MNT"
cp "$SRC" "$MNT/remboot.conf"
sync
echo "-- volume root --"
ls -la "$MNT" | grep -iE "remboot.conf|\.iso" | head
umount "$MNT"
losetup -d "$LOOP"
echo "DONE: remboot.conf written into $IMG"
