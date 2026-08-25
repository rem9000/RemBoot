#!/bin/bash
# Live, interactive QEMU window on the Windows desktop (via WSLg).
# Close the window (or press ESC in the menu) to quit.
set -e
DATA_IMG=/root/remboot-data.img
DATA_DRIVE=()
[ -f "$DATA_IMG" ] && DATA_DRIVE=(-drive "format=raw,snapshot=on,file=$DATA_IMG")
cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/remboot-vars-live.fd
# WinPE ISOs (Hiren's/HBCD) need several GB for their boot.wim ramdisk.
MEM=${REMBOOT_MEM:-6144}
exec qemu-system-x86_64 -enable-kvm -m "$MEM" -machine q35 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
  -drive if=pflash,format=raw,file=/tmp/remboot-vars-live.fd \
  -drive format=raw,file=/root/remboot-esp.img \
  "${DATA_DRIVE[@]}" \
  -debugcon file:/root/debugcon-live.log \
  -name RemBoot -display gtk,zoom-to-fit=on
