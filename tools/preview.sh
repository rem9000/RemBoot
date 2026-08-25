#!/bin/bash
# Render key animation frames on the host (no QEMU) -> shots/preview_*.png
set -e
source /root/.cargo/env
export CARGO_TARGET_DIR=/root/remboot-target
cd /mnt/c/GitHub/RemBoot
rm -rf /root/preview-frames
mkdir -p /root/preview-frames
cargo run --release -q -p remboot-core --example preview -- /root/preview-frames 1280 800
mkdir -p shots
rm -f shots/preview_*.png
for p in /root/preview-frames/*.ppm; do
  python3 tools/ppm2png.py "$p" "shots/preview_$(basename "${p%.ppm}").png"
done
ls shots/
