#!/bin/bash
# M5: real ISO entries from the boot volume + confirm screen round-trip.
sleep 12
echo screendump /root/shots/m5_a_menu.ppm
sleep 0.3
echo sendkey down
sleep 0.3
echo sendkey down
sleep 0.5
echo sendkey ret
sleep 0.6
echo screendump /root/shots/m5_b_confirm.ppm
sleep 0.3
echo sendkey esc
sleep 0.6
echo screendump /root/shots/m5_c_back.ppm
sleep 0.3
echo quit
