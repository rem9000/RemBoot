#!/bin/bash
# M0 environment check — run inside WSL Ubuntu as root.
set -e
echo "== rust =="
source /root/.cargo/env
rustc --version
cargo --version
rustup target list --installed | grep -q x86_64-unknown-uefi && echo "target x86_64-unknown-uefi: OK" || echo "target x86_64-unknown-uefi: MISSING"
echo "== qemu =="
qemu-system-x86_64 --version | head -1
[ -e /dev/kvm ] && echo "KVM: OK" || echo "KVM: MISSING"
echo "== ovmf =="
ls -l /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_VARS_4M.fd
echo "== python =="
python3 --version
echo "== project mount =="
ls /mnt/c/GitHub/RemBoot
echo "== uefi float ABI =="
rustc --print cfg --target x86_64-unknown-uefi | grep -E "target_feature|soft" || true
echo "ENV CHECK DONE"
