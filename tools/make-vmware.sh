#!/bin/bash
# Build the VMware package into vmware/: an ESP disk (FAT32, app only), the
# exFAT data disk with the real ISO collection (if built), and a generated
# RemBoot.vmx referencing them.
set -e
command -v mformat >/dev/null 2>&1 || { echo "installing mtools..."; DEBIAN_FRONTEND=noninteractive apt-get install -y mtools >/dev/null; }

OUT=/root/remboot-vmware
DATA_IMG=/root/remboot-data.img
VMW=/mnt/c/GitHub/RemBoot/vmware
rm -rf "$OUT"
mkdir -p "$OUT" "$VMW"
ESP_IMG="$OUT/remboot-esp.img"

truncate -s 64M "$ESP_IMG"
printf 'label: dos\nstart=2048, type=ef, bootable\n' | sfdisk -q "$ESP_IMG"
mformat -i "$ESP_IMG"@@1M -F -v REMBOOT ::
mmd -i "$ESP_IMG"@@1M ::/EFI ::/EFI/BOOT
mcopy -i "$ESP_IMG"@@1M /root/esp/EFI/BOOT/BOOTX64.EFI ::/EFI/BOOT/BOOTX64.EFI
qemu-img convert -f raw -O vmdk "$ESP_IMG" "$VMW/remboot.vmdk"

DATA_LINES=""
if [ -f "$DATA_IMG" ]; then
  echo "converting exFAT data disk to VMDK (copies the full ISO payload)..."
  qemu-img convert -f raw -O vmdk "$DATA_IMG" "$VMW/remboot-data.vmdk"
  DATA_LINES='sata0:1.present = "TRUE"
sata0:1.fileName = "remboot-data.vmdk"
sata0:1.deviceType = "disk"'
else
  echo "NOTE: $DATA_IMG missing — run tools/make-data-image.sh first for the ISO data disk."
fi

cat > "$VMW/RemBoot.vmx" <<EOF
.encoding = "UTF-8"
config.version = "8"
virtualHW.version = "18"
displayName = "RemBoot"
guestOS = "other-64"
firmware = "efi"
efi.secureBoot.enabled = "FALSE"
memsize = "6144"
numvcpus = "2"
sata0.present = "TRUE"
sata0:0.present = "TRUE"
sata0:0.fileName = "remboot.vmdk"
sata0:0.deviceType = "disk"
$DATA_LINES
ethernet0.present = "FALSE"
sound.present = "FALSE"
floppy0.present = "FALSE"
usb.present = "TRUE"
tools.syncTime = "FALSE"
msg.autoanswer = "TRUE"
uuid.action = "create"
EOF

ls -la "$VMW"
