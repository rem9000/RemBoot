#!/bin/bash
# Actually boot an ISO: navigate to memtest (small, fast El Torito EFI) and
# chainload it. Screenshots before and well after the handoff.
sleep 12
echo screendump /root/shots/boot_a_menu.ppm
sleep 0.3
# menu is sorted; memtest.iso is entry 04 (index 3): three downs.
echo sendkey down
sleep 0.25
echo sendkey down
sleep 0.25
echo sendkey down
sleep 0.5
echo screendump /root/shots/boot_b_selected.ppm
sleep 0.2
echo sendkey ret
sleep 0.6
echo screendump /root/shots/boot_c_confirm.ppm
sleep 0.2
echo sendkey ret
sleep 4
echo screendump /root/shots/boot_d_after.ppm
sleep 3
echo screendump /root/shots/boot_e_later.ppm
sleep 2
echo quit
