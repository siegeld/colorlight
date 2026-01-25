#!/usr/bin/env python3
"""
Full-frame test for HUB75 LED panel - avoids flickering by always sending complete frames
"""
import socket
import struct
import time

UDP_IP = '10.11.6.250'
UDP_PORT = 26177  # Port for panel 0

# Panel dimensions
NUM_COLS = 128
NUM_ROWS = 64

# Framebuffer - stores RGB values for each pixel
framebuffer = [[(0, 0, 0) for _ in range(NUM_COLS)] for _ in range(NUM_ROWS)]

def make_pixel(x, y, r, g, b):
    """Create pixel data word for UDP packet"""
    addr = ((y & 0x3F) << 7) | (x & 0x7F)
    r6 = (r >> 2) & 0x3F
    g6 = (g >> 2) & 0x3F
    b6 = (b >> 2) & 0x3F
    return (addr << 18) | (r6 << 12) | (g6 << 6) | b6

def send_full_frame(sock):
    """Send the entire framebuffer to the panel"""
    for y in range(NUM_ROWS):
        data = bytearray()
        for x in range(NUM_COLS):
            r, g, b = framebuffer[y][x]
            pixel = make_pixel(x, y, r, g, b)
            data.extend(pixel.to_bytes(4, 'big'))
        sock.sendto(data, (UDP_IP, UDP_PORT))

def clear_framebuffer(r=0, g=0, b=0):
    """Clear framebuffer to a solid color"""
    for y in range(NUM_ROWS):
        for x in range(NUM_COLS):
            framebuffer[y][x] = (r, g, b)

def draw_box(x0, y0, width, height, r, g, b):
    """Draw a filled box in the framebuffer"""
    for y in range(y0, min(y0 + height, NUM_ROWS)):
        for x in range(x0, min(x0 + width, NUM_COLS)):
            framebuffer[y][x] = (r, g, b)

def main():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    print(f"Full-frame test at {UDP_IP}:{UDP_PORT}")
    print(f"Panel size: {NUM_COLS}x{NUM_ROWS}")
    print()

    # Test 1: Clear to black
    print("Clearing to black...")
    clear_framebuffer(0, 0, 0)
    send_full_frame(sock)
    time.sleep(1)

    # Test 2: 50x50 red box at origin
    print("Drawing 50x50 red box at (0,0)...")
    clear_framebuffer(0, 0, 0)
    draw_box(0, 0, 50, 50, 255, 0, 0)
    send_full_frame(sock)
    time.sleep(2)

    # Test 3: 10x10 red box at origin
    print("Drawing 10x10 red box at (0,0)...")
    clear_framebuffer(0, 0, 0)
    draw_box(0, 0, 10, 10, 255, 0, 0)
    send_full_frame(sock)
    time.sleep(2)

    # Test 4: 50x10 red box
    print("Drawing 50x10 red box at (0,0)...")
    clear_framebuffer(0, 0, 0)
    draw_box(0, 0, 50, 10, 255, 0, 0)
    send_full_frame(sock)
    time.sleep(2)

    # Test 5: Move box animation
    print("Moving red box animation...")
    for frame in range(78):  # Move from x=0 to x=78 (128-50)
        clear_framebuffer(0, 0, 0)
        draw_box(frame, 10, 50, 40, 255, 0, 0)
        send_full_frame(sock)
        time.sleep(0.05)

    # Test 6: Final clear
    print("Final clear to black...")
    clear_framebuffer(0, 0, 0)
    send_full_frame(sock)

    print("Test complete!")
    sock.close()

if __name__ == "__main__":
    main()
