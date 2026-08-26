#!/bin/bash
sleep 4;  echo sendkey ret
sleep 3;  echo sendkey ret
sleep 2;  echo sendkey down; echo sendkey ret       # Enroll key from disk
sleep 2;  echo sendkey ret                            # volume
sleep 2;  echo sendkey ret                            # EFI/
sleep 2;  echo sendkey down; echo sendkey ret         # BOOT/
sleep 2;  echo sendkey down; echo sendkey down; echo sendkey down; echo sendkey down; echo sendkey ret  # cert
sleep 2;  echo sendkey down; echo sendkey ret         # Continue
sleep 2;  echo sendkey down; echo sendkey ret         # Yes (enroll)
sleep 3;  echo screendump /root/shots/enrolled.ppm
sleep 2;  echo quit
