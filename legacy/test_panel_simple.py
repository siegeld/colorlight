#!/usr/bin/env python3
"""
Simple test for HUB75 LED panel - send one pixel at a time
"""
import socket
import struct
import time

UDP_IP = '10.11.6.250'
UDP_PORT = 0x6601  # Port with bit 0 set for panel 0

# Panel dimensions
NUM_COLS = 128
NUM_ROWS = 64

def send_pixel(sock, x, y, r, g, b):
    """Send a single pixel using original protocol format"""
    # For 128-wide panel, addr needs 7 bits for x
    addr = ((y & 0x3F) << 7) | (x & 0x7F)

    # Pack: addr[13:0] in [31:18], R[5:0] in [17:12], G[5:0] in [11:6], B[5:0] in [5:0]
    r6 = (r >> 2) & 0x3F
    g6 = (g >> 2) & 0x3F
    b6 = (b >> 2) & 0x3F

    pixel = (addr << 18) | (r6 << 12) | (g6 << 6) | b6

    # Send as network byte order (big endian)
    data = struct.pack('>I', pixel)
    sock.sendto(data, (UDP_IP, UDP_PORT))

def fill_screen(sock, r, g, b):
    """Fill screen with solid color, one pixel at a time"""
    print(f"Filling with RGB({r},{g},{b})...")
    for y in range(NUM_ROWS):
        for x in range(NUM_COLS):
            send_pixel(sock, x, y, r, g, b)
        # Small delay between rows
        time.sleep(0.001)

def main():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    print(f"Testing panel at {UDP_IP}:{UDP_PORT}")
    print(f"Panel size: {NUM_COLS}x{NUM_ROWS}")
    print()

    # Try filling with red
    fill_screen(sock, 255, 0, 0)
    print("Sent red - do you see it?")
    time.sleep(2)

    # Try green
    fill_screen(sock, 0, 255, 0)
    print("Sent green")
    time.sleep(2)

    # Try blue
    fill_screen(sock, 0, 0, 255)
    print("Sent blue")
    time.sleep(2)

    # White
    fill_screen(sock, 255, 255, 255)
    print("Sent white")

    sock.close()

if __name__ == "__main__":
    main()
