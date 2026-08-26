#!/bin/bash
# Boot the Secure-Boot chain with SB-enabled OVMF + Microsoft keys enrolled.
#   usage: run-qemu-sb.sh <scenario.sh>
set -e
PROJ=/mnt/c/GitHub/RemBoot
SHOTS=/root/shots; rm -rf "$SHOTS"; mkdir -p "$SHOTS"
# .ms VARS ships with PK/KEK/db = Microsoft keys (so shim is trusted). Reuse
# the file across runs so a MOK enrolment in one run persists to the next;
# REMBOOT_SB_FRESH=1 starts over with pristine firmware variables.
VARS=/tmp/remboot-vars-sb.fd
if [ "${REMBOOT_SB_FRESH:-0}" = 1 ] || [ ! -f "$VARS" ]; then
  cp /usr/share/OVMF/OVMF_VARS_4M.ms.fd "$VARS"
fi
DATA_IMG=/root/remboot-data.img
DATA=(); [ -f "$DATA_IMG" ] && DATA=(-drive "format=raw,snapshot=on,file=$DATA_IMG")

bash "$1" | qemu-system-x86_64 -enable-kvm -m 6144 -machine q35 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.secboot.fd \
  -drive if=pflash,format=raw,file=/tmp/remboot-vars-sb.fd \
  -drive format=raw,file=/root/remboot-esp-sb.img \
  "${DATA[@]}" \
  -debugcon file:/root/debugcon-sb.log \
  -display none -monitor stdio -serial none > /root/qemu-sb.log 2>&1

shopt -s nullglob
rm -rf "$PROJ/shots"; mkdir -p "$PROJ/shots"
for p in "$SHOTS"/*.ppm; do
  python3 "$PROJ/tools/ppm2png.py" "$p" "$PROJ/shots/$(basename "${p%.ppm}").png"
done
ls "$PROJ/shots"
