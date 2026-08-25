#!/bin/bash
# Reproduce: boot Hiren's BootCD PE (entry 07 with the example config) and see
# what actually appears.
sleep 12
echo screendump /root/shots/hb_menu.ppm
sleep 0.3
for i in 1 2 3 4 5 6; do echo sendkey down; sleep 0.25; done
sleep 0.4
echo screendump /root/shots/hb_selected.ppm
sleep 0.2
echo sendkey ret
sleep 0.6
echo sendkey ret
sleep 6
echo screendump /root/shots/hb_a.ppm
sleep 8
echo screendump /root/shots/hb_b.ppm
sleep 10
echo screendump /root/shots/hb_c.ppm
sleep 2
echo quit
