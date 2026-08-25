#!/bin/bash
# M4: idle shot, then sendkey down and catch the bar mid-flight + settled.
sleep 12
echo screendump /root/shots/m4_a_idle.ppm
sleep 0.4
echo sendkey down
sleep 0.1
echo screendump /root/shots/m4_b_flight1.ppm
sleep 0.15
echo screendump /root/shots/m4_c_flight2.ppm
sleep 0.5
echo screendump /root/shots/m4_d_settled.ppm
sleep 0.3
echo sendkey down
sleep 0.3
echo sendkey down
sleep 0.6
echo screendump /root/shots/m4_e_third.ppm
sleep 0.3
echo sendkey up
sleep 0.6
echo screendump /root/shots/m4_f_back_up.ppm
sleep 0.5
echo quit
