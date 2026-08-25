#!/bin/bash
# In-menu config editor: open editor on entry 01, change VERSION to TEST, save.
sleep 12
echo sendkey e
sleep 0.5
echo screendump /root/shots/edit_a_open.ppm
sleep 0.3
echo sendkey down          # move to VERSION field
sleep 0.3
echo sendkey backspace
sleep 0.1
echo sendkey backspace
sleep 0.1
echo sendkey backspace
sleep 0.1
echo sendkey backspace
sleep 0.2
echo sendkey t
sleep 0.1
echo sendkey e
sleep 0.1
echo sendkey s
sleep 0.1
echo sendkey t
sleep 0.3
echo screendump /root/shots/edit_b_typed.ppm
sleep 0.3
echo sendkey ret           # save
sleep 0.8
echo screendump /root/shots/edit_c_saved.ppm
sleep 0.3
echo quit
