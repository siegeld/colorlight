#!/usr/bin/env python3
"""
Generate test images for HUB75 LED panels.
Usage: python3 gen_test_image.py --columns 96 --rows 48 --output img_data.bin
"""
import argparse
import struct

def create_test_image(columns, rows, output_path):
    """Create a test pattern for the specified panel size."""

    # Image format: 256-byte header + pixel data
    # Each pixel is 4 bytes (RGBX in little-endian)
    total_pixels = columns * rows
    data = bytearray(256 + total_pixels * 4)

    # Header
    struct.pack_into('<I', data, 0, columns)  # width
    struct.pack_into('<I', data, 4, total_pixels)  # length (total pixels)
    struct.pack_into('<I', data, 8, 0xd1581a40)  # magic
    struct.pack_into('<I', data, 12, 0xda5a0001)  # magic

    HEADER_SIZE = 256
    ROW_SIZE = columns * 4

    # Colors (little-endian RGBX)
    WHITE = struct.pack('<I', 0x00ffffff)
    RED = struct.pack('<I', 0x000000ff)
    GREEN = struct.pack('<I', 0x0000ff00)
    BLUE = struct.pack('<I', 0x00ff0000)
    YELLOW = struct.pack('<I', 0x0000ffff)
    MAGENTA = struct.pack('<I', 0x00ff00ff)
    CYAN = struct.pack('<I', 0x00ffff00)

    # Row 0 - white line at top
    for col in range(columns):
        offset = HEADER_SIZE + 0 * ROW_SIZE + col * 4
        data[offset:offset+4] = WHITE

    # Row at middle of top half (rows_per_half // 2)
    rows_per_half = rows // 2
    mid_top = rows_per_half // 2
    for col in range(columns):
        offset = HEADER_SIZE + mid_top * ROW_SIZE + col * 4
        data[offset:offset+4] = WHITE

    # Row at start of bottom half
    for col in range(columns):
        offset = HEADER_SIZE + rows_per_half * ROW_SIZE + col * 4
        data[offset:offset+4] = WHITE

    # Row at bottom - 1 (last visible row)
    for col in range(columns):
        offset = HEADER_SIZE + (rows - 2) * ROW_SIZE + col * 4
        data[offset:offset+4] = WHITE

    # Vertical lines - evenly spaced
    # Column 0: RED
    # Column columns//4: GREEN
    # Column columns//2: BLUE
    # Column 3*columns//4: YELLOW
    # Column columns-1: MAGENTA
    vertical_lines = [
        (0, RED),
        (columns // 4, GREEN),
        (columns // 2, BLUE),
        (3 * columns // 4, YELLOW),
        (columns - 1, MAGENTA),
    ]

    for col, color in vertical_lines:
        for row in range(rows):
            offset = HEADER_SIZE + row * ROW_SIZE + col * 4
            data[offset:offset+4] = color

    # Diagonal lines - X pattern
    # Top-left to bottom-right (CYAN)
    for row in range(rows):
        col = int(row * columns / rows)
        if 0 <= col < columns:
            offset = HEADER_SIZE + row * ROW_SIZE + col * 4
            data[offset:offset+4] = CYAN

    # Top-right to bottom-left (MAGENTA)
    for row in range(rows):
        col = columns - 1 - int(row * columns / rows)
        if 0 <= col < columns:
            offset = HEADER_SIZE + row * ROW_SIZE + col * 4
            data[offset:offset+4] = MAGENTA

    # Write output
    with open(output_path, 'wb') as f:
        f.write(data)

    print(f"Created test image: {output_path}")
    print(f"  Panel: {columns}x{rows}")
    print(f"  Size: {len(data)} bytes")
    print(f"  Horizontal lines at rows: 0, {mid_top}, {rows_per_half}, {rows-2}")
    print(f"  Vertical lines at cols: {[c for c, _ in vertical_lines]}")
    print(f"  Diagonal lines: X pattern (CYAN top-left to bottom-right, MAGENTA top-right to bottom-left)")

PANELS = {
    "128x64": {"columns": 128, "rows": 64},
    "96x48":  {"columns": 96,  "rows": 48},
    "64x32":  {"columns": 64,  "rows": 32},
    "64x64":  {"columns": 64,  "rows": 64},
}

def main():
    parser = argparse.ArgumentParser(description="Generate HUB75 test image")
    parser.add_argument("--panel", "-p", choices=PANELS.keys(), help="Panel type (e.g., 96x48, 128x64)")
    parser.add_argument("--columns", type=int, help="Panel columns (overrides --panel)")
    parser.add_argument("--rows", type=int, help="Panel rows (overrides --panel)")
    parser.add_argument("--output", "-o", default="img_data.bin", help="Output file (default: img_data.bin)")
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

    create_test_image(columns, rows, args.output)

if __name__ == "__main__":
    main()
