#!/usr/bin/env python3
"""Crop a region from a PPM and save as PNG (debug zoom). Usage:
   crop.py in.ppm out.png x0 y0 x1 y1 [zoom]"""
import sys

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from ppm2png import read_ppm, write_png

src, dst = sys.argv[1], sys.argv[2]
x0, y0, x1, y1 = map(int, sys.argv[3:7])
zoom = int(sys.argv[7]) if len(sys.argv) > 7 else 1
w, h, rgb = read_ppm(src)
cw, ch = x1 - x0, y1 - y0
crop = bytearray()
for y in range(y0, y1):
    row = rgb[(y * w + x0) * 3:(y * w + x1) * 3]
    zrow = bytearray()
    for x in range(cw):
        zrow += row[x * 3:x * 3 + 3] * zoom
    crop += bytes(zrow) * zoom
write_png(dst, cw * zoom, ch * zoom, bytes(crop))
print(f"{dst}: {cw*zoom}x{ch*zoom}")
