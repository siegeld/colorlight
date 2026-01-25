#!/usr/bin/env python3
import socket
import struct

UDP_IP = '10.11.6.250'
UDP_PORT = 26177

s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

# Clear entire 128x64 to black first
print("Clearing 128x64 to black...")
for y in range(64):
    row_data = bytearray()
    for x in range(128):
        addr = ((y & 0x3F) << 7) | (x & 0x7F)
        pixel = (addr << 18) | 0  # black
        row_data.extend(struct.pack('>I', pixel))
    s.sendto(row_data, (UDP_IP, UDP_PORT))

# Now send 64x64 red in left half
print("Sending 64x64 red fill (left half)...")
for y in range(64):
    row_data = bytearray()
    for x in range(64):
        addr = ((y & 0x3F) << 7) | (x & 0x7F)
        r, g, b = 255, 0, 0
        pixel = (addr << 18) | ((r & 0xFC) << 10) | ((g & 0xFC) << 4) | ((b & 0xFC) >> 2)
        row_data.extend(struct.pack('>I', pixel))
    s.sendto(row_data, (UDP_IP, UDP_PORT))

print("Done!")
