#!/bin/bash
# Host-side verification of the exFAT walker against the data image.
set -e
source /root/.cargo/env
export CARGO_TARGET_DIR=/root/remboot-target
cd /mnt/c/GitHub/RemBoot
cargo run --release -q -p remboot-core --example exfat_ls -- /root/remboot-data.img
