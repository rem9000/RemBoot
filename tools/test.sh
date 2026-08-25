#!/bin/bash
# Host-side unit tests for the pure core crate.
set -e
source /root/.cargo/env
export CARGO_TARGET_DIR=/root/remboot-target
cd /mnt/c/GitHub/RemBoot
cargo test --release -p remboot-core
