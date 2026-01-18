#!/usr/bin/env python3
"""
Plasma effect animation for Colorlight display.

Classic demoscene plasma effect using sine waves.
"""

import sys
import math
import time
sys.path.insert(0, '..')

from colorlight import ColorlightDisplay


def hsv_to_rgb(h: float, s: float, v: float) -> tuple:
    """Convert HSV to RGB (0-63 range for 6-bit color)."""
    if s == 0.0:
        r = g = b = int(v * 63)
        return (r, g, b)

    i = int(h * 6.0)
    f = (h * 6.0) - i
    p = v * (1.0 - s)
    q = v * (1.0 - s * f)
    t = v * (1.0 - s * (1.0 - f))
    i = i % 6

    if i == 0:
        r, g, b = v, t, p
    elif i == 1:
        r, g, b = q, v, p
    elif i == 2:
        r, g, b = p, v, t
    elif i == 3:
        r, g, b = p, q, v
    elif i == 4:
        r, g, b = t, p, v
    else:
        r, g, b = v, p, q

    return (int(r * 63), int(g * 63), int(b * 63))


def plasma(display: ColorlightDisplay, t: float) -> None:
    """Generate one frame of plasma effect."""
    for y in range(display.height):
        for x in range(display.width):
            # Combine multiple sine waves for plasma effect
            v1 = math.sin(x * 0.1 + t)
            v2 = math.sin((y * 0.1 + t) * 0.5)
            v3 = math.sin((x * 0.1 + y * 0.1 + t) * 0.5)
            v4 = math.sin(math.sqrt((x - 32) ** 2 + (y - 32) ** 2) * 0.15 + t)

            # Combine and normalize to 0-1
            v = (v1 + v2 + v3 + v4 + 4) / 8

            # Convert to color using HSV
            r, g, b = hsv_to_rgb(v, 1.0, 1.0)
            display.set_pixel(x, y, r, g, b)


def main():
    display = ColorlightDisplay()

    print("Running plasma effect. Press Ctrl+C to stop.")

    t = 0.0
    try:
        while True:
            plasma(display, t)
            t += 0.1
            time.sleep(0.03)  # ~30 FPS target (actual will be slower due to UDP)
    except KeyboardInterrupt:
        print("\nStopping...")

    display.clear()
    display.close()


if __name__ == "__main__":
    main()
