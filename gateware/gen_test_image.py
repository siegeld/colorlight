#!/usr/bin/env python3
"""
Generate test images for HUB75 LED panels.
Usage: python3 gen_test_image.py --panel 96x48 --pattern grid --output img_data.bin
"""
import argparse
import struct
import math

PANELS = {
    "256x64": {"columns": 256, "rows": 64},
    "128x64": {"columns": 128, "rows": 64},
    "96x48":  {"columns": 96,  "rows": 48},
    "64x32":  {"columns": 64,  "rows": 32},
    "64x64":  {"columns": 64,  "rows": 64},
}

PATTERNS = ["grid", "rainbow", "solid_white", "solid_red", "solid_green", "solid_blue"]


def create_header(columns, rows):
    """Create image header."""
    total_pixels = columns * rows
    data = bytearray(256 + total_pixels * 4)
    struct.pack_into('<I', data, 0, columns)  # width
    struct.pack_into('<I', data, 4, total_pixels)  # length (total pixels)
    struct.pack_into('<I', data, 8, 0xd1581a40)  # magic
    struct.pack_into('<I', data, 12, 0xda5a0001)  # magic
    return data


def set_pixel(data, columns, row, col, r, g, b):
    """Set a pixel at (row, col) to RGB color."""
    HEADER_SIZE = 256
    offset = HEADER_SIZE + row * columns * 4 + col * 4
    # Little-endian RGBX format: 0x00BBGGRR
    data[offset:offset+4] = struct.pack('<I', (b << 16) | (g << 8) | r)


def hsv_to_rgb(h, s, v):
    """Convert HSV to RGB. h in [0,360), s,v in [0,1]."""
    c = v * s
    x = c * (1 - abs((h / 60) % 2 - 1))
    m = v - c

    if h < 60:
        r, g, b = c, x, 0
    elif h < 120:
        r, g, b = x, c, 0
    elif h < 180:
        r, g, b = 0, c, x
    elif h < 240:
        r, g, b = 0, x, c
    elif h < 300:
        r, g, b = x, 0, c
    else:
        r, g, b = c, 0, x

    return int((r + m) * 255), int((g + m) * 255), int((b + m) * 255)


def pattern_grid(data, columns, rows):
    """Grid pattern with horizontal, vertical, and diagonal lines."""
    # Colors
    WHITE = (255, 255, 255)
    RED = (255, 0, 0)
    GREEN = (0, 255, 0)
    BLUE = (0, 0, 255)
    YELLOW = (255, 255, 0)
    MAGENTA = (255, 0, 255)
    CYAN = (0, 255, 255)

    # Horizontal lines - equally spaced
    horizontal_rows = [0, rows // 4, rows // 2, 3 * rows // 4, rows - 1]
    for row in horizontal_rows:
        for col in range(columns):
            set_pixel(data, columns, row, col, *WHITE)

    # Vertical lines - evenly spaced with colors
    vertical_lines = [
        (0, RED),
        (columns // 4, GREEN),
        (columns // 2, BLUE),
        (3 * columns // 4, YELLOW),
        (columns - 1, MAGENTA),
    ]
    for col, color in vertical_lines:
        for row in range(rows):
            set_pixel(data, columns, row, col, *color)

    # Diagonal X pattern
    for row in range(rows):
        col = int(row * columns / rows)
        if 0 <= col < columns:
            set_pixel(data, columns, row, col, *CYAN)
        col = columns - 1 - int(row * columns / rows)
        if 0 <= col < columns:
            set_pixel(data, columns, row, col, *MAGENTA)

    print(f"  Pattern: grid")
    print(f"  Horizontal lines at rows: {horizontal_rows}")
    print(f"  Vertical lines at cols: {[c for c, _ in vertical_lines]}")
    print(f"  Diagonal X pattern")


def pattern_rainbow(data, columns, rows):
    """Classic rainbow wave pattern - diagonal color waves."""
    for row in range(rows):
        for col in range(columns):
            # Create diagonal waves - hue varies with position
            # Wave moves diagonally across the panel
            hue = ((col + row) * 360 / (columns + rows) * 2) % 360
            r, g, b = hsv_to_rgb(hue, 1.0, 1.0)
            set_pixel(data, columns, row, col, r, g, b)

    print(f"  Pattern: rainbow (diagonal waves)")


def pattern_solid(data, columns, rows, r, g, b, name):
    """Solid color fill."""
    for row in range(rows):
        for col in range(columns):
            set_pixel(data, columns, row, col, r, g, b)
    print(f"  Pattern: solid {name}")


def create_test_image(columns, rows, pattern, output_path):
    """Create a test pattern for the specified panel size."""
    data = create_header(columns, rows)

    if pattern == "grid":
        pattern_grid(data, columns, rows)
    elif pattern == "rainbow":
        pattern_rainbow(data, columns, rows)
    elif pattern == "solid_white":
        pattern_solid(data, columns, rows, 255, 255, 255, "white")
    elif pattern == "solid_red":
        pattern_solid(data, columns, rows, 255, 0, 0, "red")
    elif pattern == "solid_green":
        pattern_solid(data, columns, rows, 0, 255, 0, "green")
    elif pattern == "solid_blue":
        pattern_solid(data, columns, rows, 0, 0, 255, "blue")
    else:
        raise ValueError(f"Unknown pattern: {pattern}")

    with open(output_path, 'wb') as f:
        f.write(data)

    print(f"Created test image: {output_path}")
    print(f"  Panel: {columns}x{rows}")
    print(f"  Size: {len(data)} bytes")


def main():
    parser = argparse.ArgumentParser(
        description="Generate HUB75 test images",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Available patterns:
  grid         White grid lines with colored verticals and diagonal X (default)
  rainbow      Classic rainbow diagonal wave pattern
  solid_white  Solid white (full brightness test)
  solid_red    Solid red
  solid_green  Solid green
  solid_blue   Solid blue

Examples:
  python3 gen_test_image.py --panel 96x48 --pattern grid
  python3 gen_test_image.py --panel 128x64 --pattern rainbow
  python3 gen_test_image.py -p 96x48 -t rainbow -o rainbow.bin
""")
    parser.add_argument("--panel", "-p", choices=PANELS.keys(),
                        help="Panel type (e.g., 96x48, 128x64)")
    parser.add_argument("--pattern", "-t", choices=PATTERNS, default="grid",
                        help="Test pattern (default: grid)")
    parser.add_argument("--columns", type=int,
                        help="Panel columns (overrides --panel)")
    parser.add_argument("--rows", type=int,
                        help="Panel rows (overrides --panel)")
    parser.add_argument("--output", "-o", default="img_data.bin",
                        help="Output file (default: img_data.bin)")
    args = parser.parse_args()

    # Get dimensions from --panel or explicit --columns/--rows
    if args.panel:
        columns = PANELS[args.panel]["columns"]
        rows = PANELS[args.panel]["rows"]
    else:
        columns = args.columns if args.columns else 96
        rows = args.rows if args.rows else 48

    # Allow explicit overrides
    if args.columns:
        columns = args.columns
    if args.rows:
        rows = args.rows

    create_test_image(columns, rows, args.pattern, args.output)


if __name__ == "__main__":
    main()
