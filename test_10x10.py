#!/usr/bin/env python3
"""
Send full 64x64 frame with 10x10 red box at origin
"""
import socket
import struct

UDP_IP = '10.11.6.250'
UDP_PORT = 6000

s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

print("Sending 64x64 frame with 10x10 red box at (0,0)...")

for y in range(64):
    row_data = bytearray()
    for x in range(64):
        addr = ((y & 0x3F) << 6) | (x & 0x3F)

        # Red only in 10x10 box at origin
        if x < 10 and y < 10:
            r, g, b = 255, 0, 0
        else:
            r, g, b = 0, 0, 0

        pixel = (addr << 18) | (((int(r)) & 0xFC) << 10) \
                             | (((int(g)) & 0xFC) << 4) \
                             | (((int(b)) & 0xFC) >> 2)
        row_data.extend(struct.pack('>I', pixel))
    s.sendto(row_data, (UDP_IP, UDP_PORT))

print("Done! Count the red pixels.")
