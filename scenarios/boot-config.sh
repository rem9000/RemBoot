#!/bin/bash
# With config applied, MemTest86+ is entry 03 (index 2). Verify label->iso
# boot mapping still reaches the real memtest.iso.
sleep 12
echo sendkey down
sleep 0.25
echo sendkey down
sleep 0.5
echo screendump /root/shots/cfg_a_selected.ppm
sleep 0.2
echo sendkey ret
sleep 0.5
echo sendkey ret
sleep 5
echo screendump /root/shots/cfg_b_booted.ppm
sleep 2
echo quit
