#!/bin/bash
# Animation verification: catch the intro stages, bar flight, confirm modal.
sleep 3.2
echo screendump /root/shots/q01_boot32.ppm
sleep 0.4
echo screendump /root/shots/q02_boot36.ppm
sleep 0.4
echo screendump /root/shots/q03_boot40.ppm
sleep 0.4
echo screendump /root/shots/q04_boot44.ppm
sleep 0.5
echo screendump /root/shots/q05_boot49.ppm
sleep 0.7
echo screendump /root/shots/q06_boot56.ppm
sleep 0.8
echo screendump /root/shots/q07_boot64.ppm
sleep 1.2
echo screendump /root/shots/q08_idle.ppm
sleep 0.3
echo sendkey down
sleep 0.12
echo sendkey down
sleep 0.08
echo screendump /root/shots/q09_flight.ppm
sleep 0.6
echo screendump /root/shots/q10_settled.ppm
sleep 0.2
echo sendkey ret
sleep 0.09
echo screendump /root/shots/q11_modal_mid.ppm
sleep 0.5
echo screendump /root/shots/q12_modal_open.ppm
sleep 0.3
echo sendkey esc
sleep 0.5
echo screendump /root/shots/q13_back.ppm
sleep 0.3
echo quit
