#!/bin/bash
# Dump the remboot.conf that the app wrote to its ESP image.
set -e
echo "=== /remboot.conf on the ESP image ==="
mcopy -i /root/remboot-esp.img@@1M ::/remboot.conf - 2>/dev/null || echo "(no remboot.conf on ESP)"
