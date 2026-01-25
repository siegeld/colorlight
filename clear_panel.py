#!/usr/bin/env python3
import socket
import struct

UDP_IP = '10.11.6.250'
UDP_PORT = 26177

s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

print("Clearing panel to black...")
for y in range(64):
    row_data = bytearray()
    for x in range(64):
        addr = ((y & 0x3F) << 6) | (x & 0x3F)
        pixel = (addr << 18) | 0
        row_data.extend(struct.pack('>I', pixel))
    s.sendto(row_data, (UDP_IP, UDP_PORT))

print("Done!")
