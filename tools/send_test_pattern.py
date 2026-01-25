#!/usr/bin/env python3
"""Generate and send test patterns to the LED panel via UDP (no image file needed)."""
import argparse
import colorsys
import socket
import struct
import time

HEADER_FMT = "<2sHBBHH"  # magic, frame_id, chunk_idx, total_chunks, width, height
HEADER_SIZE = struct.calcsize(HEADER_FMT)  # 10 bytes
MAX_PAYLOAD = 1462
PIXELS_PER_CHUNK = MAX_PAYLOAD // 3  # 487
BYTES_PER_CHUNK = PIXELS_PER_CHUNK * 3  # 1461 — pixel-aligned


def gradient(width, height):
    """Red-green gradient: red increases left-to-right, green increases top-to-bottom."""
    data = bytearray()
    for y in range(height):
        for x in range(width):
            r = x * 255 // max(width - 1, 1)
            g = y * 255 // max(height - 1, 1)
            b = 0
            data.extend([r, g, b])
    return bytes(data)


def color_bars(width, height):
    """Vertical color bars: white, yellow, cyan, green, magenta, red, blue, black."""
    colors = [
        (255, 255, 255),
        (255, 255, 0),
        (0, 255, 255),
        (0, 255, 0),
        (255, 0, 255),
        (255, 0, 0),
        (0, 0, 255),
        (0, 0, 0),
    ]
    data = bytearray()
    for y in range(height):
        for x in range(width):
            idx = x * len(colors) // width
            r, g, b = colors[idx]
            data.extend([r, g, b])
    return bytes(data)


def rainbow(width, height):
    """Rainbow: hue varies diagonally."""
    data = bytearray()
    for y in range(height):
        for x in range(width):
            hue = ((x + y) / (width + height)) % 1.0
            r, g, b = colorsys.hsv_to_rgb(hue, 1.0, 1.0)
            data.extend([int(r * 255), int(g * 255), int(b * 255)])
    return bytes(data)


def heart(width, height):
    """Filled red heart on black background."""
    data = bytearray()
    for y in range(height):
        # Map to math coords: x in [-1.3, 1.3], y in [-1.2, 1.9] (top=1.9)
        ny = 1.9 - 3.1 * y / (height - 1)
        for x in range(width):
            nx = 2.6 * x / (width - 1) - 1.3
            # Implicit heart: (x^2 + y^2 - 1)^3 - x^2 * y^3 <= 0
            v = (nx * nx + ny * ny - 1.0) ** 3 - nx * nx * ny * ny * ny
            if v <= 0:
                data.extend([255, 0, 0])
            else:
                data.extend([0, 0, 0])
    return bytes(data)


PATTERNS = {
    "gradient": gradient,
    "bars": color_bars,
    "rainbow": rainbow,
    "heart": heart,
}


def send_rgb_data(host, port, rgb_data, width, height, frame_id=None):
    if frame_id is None:
        frame_id = int(time.time()) & 0xFFFF
    total_chunks = (len(rgb_data) + BYTES_PER_CHUNK - 1) // BYTES_PER_CHUNK
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    for i in range(total_chunks):
        offset = i * BYTES_PER_CHUNK
        chunk = rgb_data[offset : offset + BYTES_PER_CHUNK]
        header = struct.pack(
            HEADER_FMT, b"BM", frame_id, i, total_chunks, width, height
        )
        sock.sendto(header + chunk, (host, port))
        time.sleep(0.01)
    sock.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Send a generated test pattern to the LED panel"
    )
    parser.add_argument(
        "pattern",
        choices=list(PATTERNS.keys()),
        help="Pattern to generate",
    )
    parser.add_argument("--host", default="10.11.6.250")
    parser.add_argument("--port", type=int, default=7000)
    parser.add_argument("--width", type=int, default=96)
    parser.add_argument("--height", type=int, default=48)
    args = parser.parse_args()

    rgb_data = PATTERNS[args.pattern](args.width, args.height)
    send_rgb_data(args.host, args.port, rgb_data, args.width, args.height)
    print(f"Sent '{args.pattern}' pattern ({args.width}x{args.height})")
