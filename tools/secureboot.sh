#!/bin/bash
# Assemble a Secure-Boot-capable ESP image:
#   \EFI\BOOT\BOOTX64.EFI   = Microsoft-signed shim (boots with SB on)
#   \EFI\BOOT\mmx64.efi     = MokManager (one-time key enrolment)
#   \EFI\BOOT\grubx64.efi   = RemBoot, signed with our own MOK key
#   \EFI\BOOT\ENROLL_REMBOOT.cer = the key to enrol (DER)
#
# The private key lives in dist/keys (gitignored) and is generated once.
set -e
SHIM=/usr/lib/shim/shimx64.efi.signed
MM=/usr/lib/shim/mmx64.efi
APP=/root/esp/EFI/BOOT/BOOTX64.EFI          # built by tools/build.sh
KEYDIR=/mnt/c/GitHub/RemBoot/dist/keys
IMG=/root/remboot-esp-sb.img

command -v sbsign >/dev/null || { echo "install: apt-get install -y shim-signed sbsigntool"; exit 1; }
[ -f "$APP" ] || { echo "build the app first: tools/build.sh"; exit 1; }

mkdir -p "$KEYDIR"
if [ ! -f "$KEYDIR/MOK.key" ]; then
  echo "generating a signing key (MOK) in $KEYDIR ..."
  openssl req -new -x509 -newkey rsa:2048 -nodes -days 3650 \
    -subj "/CN=RemBoot Machine Owner Key/" \
    -keyout "$KEYDIR/MOK.key" -out "$KEYDIR/MOK.crt" 2>/dev/null
  openssl x509 -in "$KEYDIR/MOK.crt" -outform DER -out "$KEYDIR/MOK.cer"
fi

# RemBoot carries its own .sbat section (see efi/src/main.rs) so shim accepts
# it under Secure Boot. Just sign the built app.
echo "signing RemBoot -> grubx64.efi ..."
sbsign --key "$KEYDIR/MOK.key" --cert "$KEYDIR/MOK.crt" --output /root/grubx64.efi "$APP"
sbverify --cert "$KEYDIR/MOK.crt" /root/grubx64.efi

echo "building Secure-Boot ESP image ..."
rm -f "$IMG"; truncate -s 96M "$IMG"
printf 'label: dos\nstart=2048, type=ef, bootable\n' | sfdisk -q "$IMG"
mformat -i "$IMG"@@1M -F -v REMBOOT ::
mmd -i "$IMG"@@1M ::/EFI ::/EFI/BOOT
mcopy -i "$IMG"@@1M "$SHIM" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$IMG"@@1M "$MM"   ::/EFI/BOOT/mmx64.efi
mcopy -i "$IMG"@@1M /root/grubx64.efi ::/EFI/BOOT/grubx64.efi
mcopy -i "$IMG"@@1M "$KEYDIR/MOK.cer"  ::/EFI/BOOT/ENROLL_REMBOOT.cer
echo "-- SB ESP contents --"
mdir -i "$IMG"@@1M ::/EFI/BOOT/
