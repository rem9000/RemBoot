#!/usr/bin/env python3
"""Minimal PPM(P6) -> PNG converter, stdlib only."""
import sys, zlib, struct

def read_ppm(path):
    with open(path, 'rb') as f:
        data = f.read()
    # parse header: P6 <w> <h> <maxval>\n
    parts = []
    i = 0
    while len(parts) < 4:
        while i < len(data) and data[i:i+1].isspace():
            i += 1
        if data[i:i+1] == b'#':
            while data[i:i+1] != b'\n':
                i += 1
            continue
        j = i
        while j < len(data) and not data[j:j+1].isspace():
            j += 1
        parts.append(data[i:j])
        i = j
    i += 1  # single whitespace after maxval
    w, h = int(parts[1]), int(parts[2])
    return w, h, data[i:i + w * h * 3]

def write_png(path, w, h, rgb):
    def chunk(typ, payload):
        c = struct.pack('>I', len(payload)) + typ + payload
        return c + struct.pack('>I', zlib.crc32(typ + payload) & 0xffffffff)
    raw = b''.join(b'\x00' + rgb[y*w*3:(y+1)*w*3] for y in range(h))
    png = (b'\x89PNG\r\n\x1a\n'
           + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(raw, 9))
           + chunk(b'IEND', b''))
    with open(path, 'wb') as f:
        f.write(png)

if __name__ == '__main__':
    src, dst = sys.argv[1], sys.argv[2]
    w, h, rgb = read_ppm(src)
    write_png(dst, w, h, rgb)
    print(f'{dst}: {w}x{h}')
