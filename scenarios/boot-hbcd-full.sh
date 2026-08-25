#!/bin/bash
# Let Hiren's PE finish loading to the desktop (needs REMBOOT_MEM=8192).
sleep 12
for i in 1 2 3 4 5 6; do echo sendkey down; sleep 0.25; done
sleep 0.4
echo sendkey ret
sleep 0.6
echo sendkey ret
sleep 45
echo screendump /root/shots/hbf_a.ppm
sleep 30
echo screendump /root/shots/hbf_b.ppm
sleep 30
echo screendump /root/shots/hbf_c.ppm
sleep 2
echo quit
