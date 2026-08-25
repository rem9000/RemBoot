#!/bin/bash
# Sanity-boot the VMware disk pair (ESP + exFAT data) in QEMU + screendump.
set -e
DATA_IMG=/root/remboot-data.img
DATA_DRIVE=()
[ -f "$DATA_IMG" ] && DATA_DRIVE=(-drive "format=raw,snapshot=on,file=$DATA_IMG")
cp /usr/share/OVMF/OVMF_VARS_4M.fd /tmp/remboot-vars-vmd.fd
mkdir -p /root/shots
(
  sleep 9
  echo screendump /root/shots/vmdisk.ppm
  sleep 0.5
  echo quit
) | qemu-system-x86_64 -enable-kvm -m 512 -machine q35 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
  -drive if=pflash,format=raw,file=/tmp/remboot-vars-vmd.fd \
  -drive format=raw,file=/root/remboot-vmware/remboot-esp.img \
  "${DATA_DRIVE[@]}" \
  -debugcon file:/root/debugcon-vmd.log \
  -display none -monitor stdio -serial none > /root/qemu-mon-vmd.log 2>&1
python3 /mnt/c/GitHub/RemBoot/tools/ppm2png.py /root/shots/vmdisk.ppm /mnt/c/GitHub/RemBoot/shots/vmdisk.png
tail -6 /root/debugcon-vmd.log
