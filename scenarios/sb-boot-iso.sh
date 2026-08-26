#!/bin/bash
# RemBoot already boots under SB (MOK enrolled). Try chainloading an ISO
# (gparted, entry 05 = signed bootloader) under Secure Boot.
sleep 16                                   # shim -> RemBoot menu
for i in 1 2 3 4; do echo sendkey down; sleep 0.25; done
sleep 0.5; echo sendkey ret                # confirm
sleep 0.6; echo sendkey ret                # boot
sleep 12; echo screendump /root/shots/sbiso_a.ppm
sleep 10; echo screendump /root/shots/sbiso_b.ppm
sleep 2;  echo quit
