#!/usr/bin/env python3
"""Continuously send pattern to see if panel updates"""
import socket
import struct
import time

UDP_IP = '10.11.6.250'
UDP_PORT = 0x6601

def send_pixel(sock, x, y, r, g, b):
    addr = ((y & 0x3F) << 7) | (x & 0x7F)
    r6 = (r >> 2) & 0x3F
    g6 = (g >> 2) & 0x3F
    b6 = (b >> 2) & 0x3F
    pixel = (addr << 18) | (r6 << 12) | (g6 << 6) | b6
    data = struct.pack('>I', pixel)
    sock.sendto(data, (UDP_IP, UDP_PORT))

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

print("Sending alternating colors - watch for changes...")
print("Press Ctrl+C to stop")

colors = [(255,0,0), (0,255,0), (0,0,255), (255,255,255)]
frame = 0

try:
    while True:
        r, g, b = colors[frame % len(colors)]
        print(f"Frame {frame}: RGB({r},{g},{b})")

        # Fill 25x25 box
        for y in range(25):
            for x in range(25):
                send_pixel(sock, x, y, r, g, b)

        frame += 1
        time.sleep(1)
except KeyboardInterrupt:
    print("\nStopped")

sock.close()
