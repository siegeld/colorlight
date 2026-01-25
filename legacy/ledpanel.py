#!/usr/bin/env python3
"""
LED Panel Library for Colorlight 5A-75E

Controls HUB75 LED panels via UDP over Ethernet.
"""

import socket
import struct
from typing import Optional, Tuple, Union

try:
    from PIL import Image
    HAS_PIL = True
except ImportError:
    HAS_PIL = False

try:
    import numpy as np
    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False


class LEDPanel:
    """Control an LED panel connected via Colorlight 5A-75E FPGA."""

    def __init__(self, ip: str = '10.11.6.250', port: int = 26177,
                 width: int = 128, height: int = 64):
        """
        Initialize panel connection.

        Args:
            ip: IP address of the FPGA board
            port: UDP port number
            width: Panel width in pixels
            height: Panel height in pixels
        """
        self._ip = ip
        self._port = port
        self._width = width
        self._height = height
        self._socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

        # Frame buffer: stores (r, g, b) tuples
        self._buffer = [[(0, 0, 0) for _ in range(width)] for _ in range(height)]

    @property
    def width(self) -> int:
        """Panel width in pixels."""
        return self._width

    @property
    def height(self) -> int:
        """Panel height in pixels."""
        return self._height

    @property
    def ip(self) -> str:
        """Panel IP address."""
        return self._ip

    @property
    def port(self) -> int:
        """Panel UDP port."""
        return self._port

    def _encode_pixel(self, x: int, y: int, r: int, g: int, b: int) -> bytes:
        """Encode a pixel into the 32-bit wire format."""
        addr = ((y & 0x3F) << 7) | (x & 0x7F)
        pixel = ((addr << 18) |
                 ((r & 0xFC) << 10) |
                 ((g & 0xFC) << 4) |
                 ((b & 0xFC) >> 2))
        return struct.pack('>I', pixel)

    def set_pixel(self, x: int, y: int, r: int, g: int, b: int) -> None:
        """
        Set a single pixel in the buffer.

        Args:
            x: X coordinate (0 to width-1)
            y: Y coordinate (0 to height-1)
            r: Red component (0-255)
            g: Green component (0-255)
            b: Blue component (0-255)
        """
        if 0 <= x < self._width and 0 <= y < self._height:
            self._buffer[y][x] = (r, g, b)

    def get_pixel(self, x: int, y: int) -> Tuple[int, int, int]:
        """
        Get pixel value from buffer.

        Args:
            x: X coordinate
            y: Y coordinate

        Returns:
            Tuple of (r, g, b) values
        """
        if 0 <= x < self._width and 0 <= y < self._height:
            return self._buffer[y][x]
        return (0, 0, 0)

    def fill(self, r: int, g: int, b: int) -> None:
        """
        Fill entire panel with a single color.

        Args:
            r: Red component (0-255)
            g: Green component (0-255)
            b: Blue component (0-255)
        """
        for y in range(self._height):
            for x in range(self._width):
                self._buffer[y][x] = (r, g, b)

    def clear(self) -> None:
        """Clear panel to black."""
        self.fill(0, 0, 0)

    def show(self) -> None:
        """Send the buffer to the panel."""
        for y in range(self._height):
            row_data = bytearray()
            for x in range(self._width):
                r, g, b = self._buffer[y][x]
                row_data.extend(self._encode_pixel(x, y, r, g, b))
            self._socket.sendto(row_data, (self._ip, self._port))

    def draw_rect(self, x: int, y: int, w: int, h: int,
                  r: int, g: int, b: int, filled: bool = True) -> None:
        """
        Draw a rectangle.

        Args:
            x: Top-left X coordinate
            y: Top-left Y coordinate
            w: Width
            h: Height
            r, g, b: Color components (0-255)
            filled: If True, fill the rectangle; otherwise draw outline only
        """
        if filled:
            for py in range(y, y + h):
                for px in range(x, x + w):
                    self.set_pixel(px, py, r, g, b)
        else:
            # Top and bottom edges
            for px in range(x, x + w):
                self.set_pixel(px, y, r, g, b)
                self.set_pixel(px, y + h - 1, r, g, b)
            # Left and right edges
            for py in range(y, y + h):
                self.set_pixel(x, py, r, g, b)
                self.set_pixel(x + w - 1, py, r, g, b)

    def draw_line(self, x1: int, y1: int, x2: int, y2: int,
                  r: int, g: int, b: int) -> None:
        """
        Draw a line using Bresenham's algorithm.

        Args:
            x1, y1: Start point
            x2, y2: End point
            r, g, b: Color components (0-255)
        """
        dx = abs(x2 - x1)
        dy = abs(y2 - y1)
        sx = 1 if x1 < x2 else -1
        sy = 1 if y1 < y2 else -1
        err = dx - dy

        x, y = x1, y1
        while True:
            self.set_pixel(x, y, r, g, b)
            if x == x2 and y == y2:
                break
            e2 = 2 * err
            if e2 > -dy:
                err -= dy
                x += sx
            if e2 < dx:
                err += dx
                y += sy

    def set_image(self, image, x: int = 0, y: int = 0) -> None:
        """
        Blit an image to the buffer.

        Args:
            image: PIL Image or numpy array (RGB or RGBA)
            x: X offset for placement
            y: Y offset for placement
        """
        if HAS_PIL and isinstance(image, Image.Image):
            # Convert PIL Image to RGB
            if image.mode != 'RGB':
                image = image.convert('RGB')
            for py in range(image.height):
                if y + py >= self._height:
                    break
                for px in range(image.width):
                    if x + px >= self._width:
                        break
                    r, g, b = image.getpixel((px, py))
                    self.set_pixel(x + px, y + py, r, g, b)

        elif HAS_NUMPY and isinstance(image, np.ndarray):
            # Handle numpy array
            h, w = image.shape[:2]
            for py in range(h):
                if y + py >= self._height:
                    break
                for px in range(w):
                    if x + px >= self._width:
                        break
                    pixel = image[py, px]
                    if len(pixel) >= 3:
                        r, g, b = int(pixel[0]), int(pixel[1]), int(pixel[2])
                        self.set_pixel(x + px, y + py, r, g, b)
        else:
            raise TypeError("image must be PIL Image or numpy array")

    def __enter__(self) -> 'LEDPanel':
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        """Context manager exit - automatically calls show()."""
        self.show()

    def close(self) -> None:
        """Close the UDP socket."""
        self._socket.close()


# Convenience functions for quick usage
def fill(r: int, g: int, b: int, ip: str = '10.11.6.250', port: int = 26177) -> None:
    """Quick fill entire panel with a color."""
    panel = LEDPanel(ip=ip, port=port)
    panel.fill(r, g, b)
    panel.show()


def clear(ip: str = '10.11.6.250', port: int = 26177) -> None:
    """Quick clear panel to black."""
    fill(0, 0, 0, ip=ip, port=port)


if __name__ == '__main__':
    # Demo: color bars
    import time

    print("LED Panel Demo")
    panel = LEDPanel()

    print("Clearing to black...")
    panel.clear()
    panel.show()
    time.sleep(1)

    print("Drawing color bars...")
    bar_width = panel.width // 3
    panel.draw_rect(0, 0, bar_width, panel.height, 255, 0, 0)  # Red
    panel.draw_rect(bar_width, 0, bar_width, panel.height, 0, 255, 0)  # Green
    panel.draw_rect(bar_width * 2, 0, bar_width + 2, panel.height, 0, 0, 255)  # Blue
    panel.show()
    time.sleep(2)

    print("Drawing white border...")
    panel.draw_rect(0, 0, panel.width, panel.height, 255, 255, 255, filled=False)
    panel.show()
    time.sleep(2)

    print("Drawing diagonal lines...")
    panel.clear()
    panel.draw_line(0, 0, panel.width - 1, panel.height - 1, 255, 255, 0)
    panel.draw_line(panel.width - 1, 0, 0, panel.height - 1, 0, 255, 255)
    panel.show()
    time.sleep(2)

    print("Clearing...")
    panel.clear()
    panel.show()
    print("Done!")
