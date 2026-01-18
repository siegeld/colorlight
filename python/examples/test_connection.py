#!/usr/bin/env python3
"""
Test connection to Colorlight display.

Draws a simple pattern to verify the display is working.
"""

import sys
import time
sys.path.insert(0, '..')

from colorlight import ColorlightDisplay


def main():
    # Uses default IP from colorlight.py (192.168.178.50)
    display = ColorlightDisplay()

    print("Testing connection to Colorlight display...")
    print("You should see patterns on your LED panel.")

    # Test 1: Single red pixel
    print("1. Drawing red pixel at (0, 0)")
    display.set_pixel(0, 0, 63, 0, 0)
    time.sleep(1)

    # Test 2: Green pixel
    print("2. Drawing green pixel at (10, 10)")
    display.set_pixel(10, 10, 0, 63, 0)
    time.sleep(1)

    # Test 3: Blue pixel
    print("3. Drawing blue pixel at (20, 20)")
    display.set_pixel(20, 20, 0, 0, 63)
    time.sleep(1)

    # Test 4: White diagonal line
    print("4. Drawing white diagonal line")
    for i in range(64):
        display.set_pixel(i, i, 63, 63, 63)
    time.sleep(2)

    # Test 5: Color bars
    print("5. Drawing color bars")
    colors = [
        (63, 0, 0),   # Red
        (63, 63, 0),  # Yellow
        (0, 63, 0),   # Green
        (0, 63, 63),  # Cyan
        (0, 0, 63),   # Blue
        (63, 0, 63),  # Magenta
        (63, 63, 63), # White
        (0, 0, 0),    # Black
    ]

    bar_width = 64 // len(colors)
    for i, color in enumerate(colors):
        for x in range(i * bar_width, (i + 1) * bar_width):
            for y in range(64):
                display.set_pixel(x, y, *color)

    print("Test complete! If you saw the patterns, your connection is working.")
    print("Press Ctrl+C to exit and clear display, or wait 10 seconds.")

    try:
        time.sleep(10)
    except KeyboardInterrupt:
        pass

    print("Clearing display...")
    display.clear()
    display.close()


if __name__ == "__main__":
    main()
