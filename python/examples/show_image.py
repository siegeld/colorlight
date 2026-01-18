#!/usr/bin/env python3
"""
Display an image on the Colorlight LED panel.

Usage:
    python show_image.py path/to/image.png
    python show_image.py path/to/image.jpg --loop
"""

import sys
import argparse
import time
sys.path.insert(0, '..')

from colorlight import ColorlightFrameBuffer


def main():
    parser = argparse.ArgumentParser(description="Display image on LED panel")
    parser.add_argument("image", help="Path to image file")
    parser.add_argument("--ip", default="192.168.178.50", help="Colorlight IP address")
    parser.add_argument("--loop", action="store_true", help="Continuously refresh")
    parser.add_argument("--interval", type=float, default=1.0, help="Refresh interval in seconds")
    args = parser.parse_args()

    try:
        fb = ColorlightFrameBuffer(ip=args.ip)
    except ImportError as e:
        print(f"Error: {e}")
        print("Install numpy: pip install numpy")
        sys.exit(1)

    try:
        fb.load_image(args.image)
    except ImportError:
        print("Error: PIL/Pillow required for image loading")
        print("Install: pip install Pillow")
        sys.exit(1)
    except FileNotFoundError:
        print(f"Error: Image not found: {args.image}")
        sys.exit(1)

    print(f"Displaying: {args.image}")
    fb.send()

    if args.loop:
        print(f"Refreshing every {args.interval}s. Press Ctrl+C to stop.")
        try:
            while True:
                time.sleep(args.interval)
                fb.send()
        except KeyboardInterrupt:
            print("\nStopping...")
            fb.clear()
            fb.send()

    fb.close()


if __name__ == "__main__":
    main()
