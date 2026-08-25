#!/bin/bash
# Real ISO collection: list check + navigation + confirm with a long name.
sleep 12
echo screendump /root/shots/real_a_menu.ppm
sleep 0.3
echo sendkey down
sleep 0.5
echo screendump /root/shots/real_b_second.ppm
sleep 0.2
echo sendkey ret
sleep 0.6
echo screendump /root/shots/real_c_confirm.ppm
sleep 0.3
echo quit
