#!/usr/bin/env python3
"""
Test using exact format from matrix_video_player.py (known working code)
64x64 panel, port 6000
"""
import socket
import struct
import time

UDP_IP = '10.11.6.250'
UDP_PORT = 6000  # Original port from liteeth.yml

num_rows = 64
num_cols = 64

s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

def send_frame(r, g, b):
    """Send a solid color frame using original format"""
    for y in range(num_rows):
        row_data = bytearray()
        for x in range(num_cols):
            addr = ((y & 0x3F) << 6) | (x & 0x3F)
            # Original format: htonl converts to network byte order (big-endian)
            pixel = (addr << 18) | (((int(r)) & 0xFC) << 10) \
                                 | (((int(g)) & 0xFC) << 4) \
                                 | (((int(b)) & 0xFC) >> 2)
            row_data.extend(struct.pack('>I', pixel))  # >I = big-endian unsigned int
        s.sendto(row_data, (UDP_IP, UDP_PORT))

print(f"Testing ORIGINAL format at {UDP_IP}:{UDP_PORT}")
print("64x64 panel, port 6000")
print()

print("Sending red...")
send_frame(255, 0, 0)
time.sleep(2)

print("Sending green...")
send_frame(0, 255, 0)
time.sleep(2)

print("Sending blue...")
send_frame(0, 0, 255)
time.sleep(2)

print("Sending white...")
send_frame(255, 255, 255)
time.sleep(2)

print("Sending black...")
send_frame(0, 0, 0)

print("Done!")
