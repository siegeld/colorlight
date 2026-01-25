#!/usr/bin/env python3
"""
Test pattern for 128x64 HUB75 LED panel on Colorlight 5A-75E
"""
import socket
import time

UDP_IP = '10.11.6.250'
UDP_PORT = 26177  # Port for panel 0 (0x6641)

# Panel dimensions
NUM_COLS = 128
NUM_ROWS = 64

def make_pixel(x, y, r, g, b):
    """Create pixel data word for UDP packet

    Protocol (from udp_panel_writer.v):
    - data[31:18] = ctrl_addr (14 bits)
    - data[17:12] = R (6 bits)
    - data[11:6]  = G (6 bits)
    - data[5:0]   = B (6 bits)

    For 128x64 panel: addr = (y << 7) | x
    """
    addr = ((y & 0x3F) << 7) | (x & 0x7F)
    r6 = (r >> 2) & 0x3F
    g6 = (g >> 2) & 0x3F
    b6 = (b >> 2) & 0x3F
    return (addr << 18) | (r6 << 12) | (g6 << 6) | b6

def send_row(sock, y, pixels):
    """Send a full row of pixels as one UDP packet"""
    # Pack pixels as big-endian 32-bit words
    data = bytearray()
    for x in range(NUM_COLS):
        r, g, b = pixels[x]
        pixel = make_pixel(x, y, r, g, b)
        data.extend(pixel.to_bytes(4, 'big'))
    sock.sendto(data, (UDP_IP, UDP_PORT))

def fill_color(sock, r, g, b):
    """Fill entire panel with solid color"""
    print(f"Filling panel with RGB({r}, {g}, {b})...")
    for y in range(NUM_ROWS):
        pixels = [(r, g, b) for _ in range(NUM_COLS)]
        send_row(sock, y, pixels)
    time.sleep(0.1)

def test_pattern_stripes(sock):
    """Draw vertical color stripes"""
    print("Drawing vertical stripes...")
    colors = [
        (255, 0, 0),    # Red
        (0, 255, 0),    # Green
        (0, 0, 255),    # Blue
        (255, 255, 0),  # Yellow
        (255, 0, 255),  # Magenta
        (0, 255, 255),  # Cyan
        (255, 255, 255),# White
        (0, 0, 0),      # Black
    ]
    stripe_width = NUM_COLS // len(colors)

    for y in range(NUM_ROWS):
        pixels = []
        for x in range(NUM_COLS):
            color_idx = min(x // stripe_width, len(colors) - 1)
            pixels.append(colors[color_idx])
        send_row(sock, y, pixels)
    time.sleep(1)

def test_pattern_gradient(sock):
    """Draw color gradient"""
    print("Drawing gradient...")
    for y in range(NUM_ROWS):
        pixels = []
        for x in range(NUM_COLS):
            r = int(x * 255 / NUM_COLS)
            g = int(y * 255 / NUM_ROWS)
            b = 128
            pixels.append((r, g, b))
        send_row(sock, y, pixels)
    time.sleep(1)

def test_pattern_checkerboard(sock, size=8):
    """Draw checkerboard pattern"""
    print(f"Drawing checkerboard (size={size})...")
    for y in range(NUM_ROWS):
        pixels = []
        for x in range(NUM_COLS):
            if ((x // size) + (y // size)) % 2 == 0:
                pixels.append((255, 255, 255))
            else:
                pixels.append((0, 0, 0))
        send_row(sock, y, pixels)
    time.sleep(1)

def main():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    print(f"Testing HUB75 panel at {UDP_IP}:{UDP_PORT}")
    print(f"Panel size: {NUM_COLS}x{NUM_ROWS}")
    print()

    # Test solid colors
    fill_color(sock, 255, 0, 0)    # Red
    time.sleep(0.5)
    fill_color(sock, 0, 255, 0)    # Green
    time.sleep(0.5)
    fill_color(sock, 0, 0, 255)    # Blue
    time.sleep(0.5)
    fill_color(sock, 255, 255, 255) # White
    time.sleep(0.5)

    # Test patterns
    test_pattern_stripes(sock)
    test_pattern_gradient(sock)
    test_pattern_checkerboard(sock)

    print("Test complete!")
    sock.close()

if __name__ == "__main__":
    main()
