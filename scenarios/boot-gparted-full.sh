#!/bin/bash
# Let gparted-live boot all the way to the live desktop: the ultimate proof
# that the virtual CD serves sustained large reads across the whole ISO.
sleep 12
echo sendkey down
sleep 0.5
echo sendkey ret
sleep 0.5
echo sendkey ret
sleep 3
echo sendkey ret
sleep 40
echo screendump /root/shots/gpfull_a.ppm
sleep 15
echo screendump /root/shots/gpfull_b.ppm
sleep 10
echo screendump /root/shots/gpfull_c.ppm
sleep 2
echo quit
