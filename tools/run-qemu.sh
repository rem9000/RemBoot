#!/bin/bash
# Headless QEMU iteration loop.
#   usage: run-qemu.sh <scenario.sh>
# The scenario script emits QEMU monitor commands on stdout (with sleeps in
# between); screendumps go to /root/shots/*.ppm and are converted to PNG in
# the project's shots/ dir so they can be viewed from Windows.
set -e
PROJ=/mnt/c/GitHub/RemBoot
SHOTS=/root/shots
rm -rf "$SHOTS"
mkdir -p "$SHOTS"
cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/remboot-vars.fd

# Real ISO collection on an exFAT data disk (Ventoy-style topology).
# Built by tools/make-data-image.sh from the OneDrive iso folder.
DATA_IMG=/root/remboot-data.img
DATA_DRIVE=()
[ -f "$DATA_IMG" ] && DATA_DRIVE=(-drive "format=raw,snapshot=on,file=$DATA_IMG")

# WinPE-based ISOs (Hiren's, HBCD) build a RAM disk from boot.wim and need
# several GB; override with REMBOOT_MEM=8192 etc.
MEM=${REMBOOT_MEM:-6144}

# Monitor stdout (echoes every typed char) goes to a log to keep output sane.
bash "$1" | qemu-system-x86_64 -enable-kvm -m "$MEM" -machine q35 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
  -drive if=pflash,format=raw,file=/tmp/remboot-vars.fd \
  -drive format=raw,file=/root/remboot-esp.img \
  "${DATA_DRIVE[@]}" \
  -debugcon file:/root/debugcon.log \
  -display none -monitor stdio -serial none > /root/qemu-mon.log 2>&1

echo "-- debugcon --"
tail -20 /root/debugcon.log 2>/dev/null || true

rm -rf "$PROJ/shots"
mkdir -p "$PROJ/shots"
shopt -s nullglob
for p in "$SHOTS"/*.ppm; do
  python3 "$PROJ/tools/ppm2png.py" "$p" "$PROJ/shots/$(basename "${p%.ppm}").png"
done
echo "-- shots --"
ls -la "$PROJ/shots"
