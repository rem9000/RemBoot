#!/bin/bash
# Build the UEFI app and stage it into the QEMU ESP directory.
# Target dir lives on the WSL ext4 filesystem — compiling onto /mnt/c is slow.
set -e
source /root/.cargo/env
export CARGO_TARGET_DIR=/root/remboot-target
cd /mnt/c/GitHub/RemBoot
cargo build --release --target x86_64-unknown-uefi -p remboot
mkdir -p /root/esp/EFI/BOOT
cp /root/remboot-target/x86_64-unknown-uefi/release/remboot.efi /root/esp/EFI/BOOT/BOOTX64.EFI
rm -f /root/esp/*.iso

# Real, writable FAT ESP image. vvfat's rw mode is unreliable, and the app now
# writes remboot.conf back to its own boot volume — so we boot from a genuine
# FAT partition instead. Preserve an existing remboot.conf across rebuilds.
command -v mformat >/dev/null 2>&1 || { DEBIAN_FRONTEND=noninteractive apt-get install -y mtools >/dev/null; }
ESP_IMG=/root/remboot-esp.img
SAVED_CONF=$(mktemp)
HAVE_CONF=0
if [ -f "$ESP_IMG" ] && mcopy -i "$ESP_IMG"@@1M ::/remboot.conf "$SAVED_CONF" 2>/dev/null; then
  HAVE_CONF=1
fi
rm -f "$ESP_IMG"
truncate -s 96M "$ESP_IMG"
printf 'label: dos\nstart=2048, type=ef, bootable\n' | sfdisk -q "$ESP_IMG"
mformat -i "$ESP_IMG"@@1M -F -v REMBOOT ::
mmd -i "$ESP_IMG"@@1M ::/EFI ::/EFI/BOOT
mcopy -i "$ESP_IMG"@@1M /root/esp/EFI/BOOT/BOOTX64.EFI ::/EFI/BOOT/BOOTX64.EFI
[ "$HAVE_CONF" = 1 ] && mcopy -i "$ESP_IMG"@@1M "$SAVED_CONF" ::/remboot.conf && echo "(preserved existing remboot.conf)"
rm -f "$SAVED_CONF"

# Stage the built app where Windows tools (tools/make-usb.ps1) can reach it.
mkdir -p /mnt/c/GitHub/RemBoot/dist/EFI/BOOT
cp /root/esp/EFI/BOOT/BOOTX64.EFI /mnt/c/GitHub/RemBoot/dist/EFI/BOOT/BOOTX64.EFI

ls -l /root/esp/EFI/BOOT/BOOTX64.EFI "$ESP_IMG"
