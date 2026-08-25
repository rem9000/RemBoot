#!/bin/bash
# Boot a GRUB-based Linux ISO (gparted-live, entry 02) to prove the chainload
# works for real distro ISOs, not just memtest. Catch the GRUB menu.
sleep 12
echo sendkey down
sleep 0.5
echo sendkey ret
sleep 0.5
echo sendkey ret
sleep 6
echo screendump /root/shots/gp_a.ppm
sleep 5
echo screendump /root/shots/gp_b.ppm
sleep 4
echo screendump /root/shots/gp_c.ppm
sleep 2
echo quit
