#!/usr/bin/env python3
"""
Colorlight 5A-75E HUB75 Display Driver

Send pixel data to Colorlight FPGA board over UDP.
"""

import socket
from typing import Tuple, Optional

try:
    import numpy as np
    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False


class ColorlightDisplay:
    """Control a HUB75 LED panel via Colorlight 5A-75E."""

    def __init__(
        self,
        ip: str = "192.168.178.50",
        base_port: int = 6000,
        width: int = 64,
        height: int = 64
    ):
        """
        Initialize display connection.

        Args:
            ip: IP address of the Colorlight board
            base_port: Base UDP port (panel 0), increments for additional panels
            width: Panel width in pixels
            height: Panel height in pixels
        """
        self.ip = ip
        self.base_port = base_port
        self.width = width
        self.height = height
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    def set_pixel(
        self,
        x: int,
        y: int,
        r: int,
        g: int,
        b: int,
        panel: int = 0
    ) -> None:
        """
        Set a single pixel.

        Args:
            x: X coordinate (0 to width-1)
            y: Y coordinate (0 to height-1)
            r: Red value (0-63 for 6-bit, or 0-255 will be scaled)
            g: Green value (0-63 for 6-bit, or 0-255 will be scaled)
            b: Blue value (0-63 for 6-bit, or 0-255 will be scaled)
            panel: Panel index (0-based)
        """
        # Scale 8-bit to 6-bit if values are > 63
        if r > 63 or g > 63 or b > 63:
            r = (r >> 2) & 0x3F
            g = (g >> 2) & 0x3F
            b = (b >> 2) & 0x3F

        data = bytes([
            y & 0x3F,
            x & 0x3F,
            r & 0x3F,
            g & 0x3F,
            b & 0x3F
        ])
        self.sock.sendto(data, (self.ip, self.base_port + panel))

    def set_pixel_rgb(
        self,
        x: int,
        y: int,
        rgb: Tuple[int, int, int],
        panel: int = 0
    ) -> None:
        """Set pixel using RGB tuple."""
        self.set_pixel(x, y, rgb[0], rgb[1], rgb[2], panel)

    def clear(self, panel: int = 0) -> None:
        """Clear display to black."""
        for y in range(self.height):
            for x in range(self.width):
                self.set_pixel(x, y, 0, 0, 0, panel)

    def fill(self, r: int, g: int, b: int, panel: int = 0) -> None:
        """Fill display with solid color."""
        for y in range(self.height):
            for x in range(self.width):
                self.set_pixel(x, y, r, g, b, panel)

    def draw_rect(
        self,
        x: int,
        y: int,
        w: int,
        h: int,
        r: int,
        g: int,
        b: int,
        filled: bool = True,
        panel: int = 0
    ) -> None:
        """Draw a rectangle."""
        for py in range(y, min(y + h, self.height)):
            for px in range(x, min(x + w, self.width)):
                if filled or py == y or py == y + h - 1 or px == x or px == x + w - 1:
                    self.set_pixel(px, py, r, g, b, panel)

    def close(self) -> None:
        """Close the UDP socket."""
        self.sock.close()


class ColorlightFrameBuffer:
    """Frame buffer for batch pixel updates."""

    def __init__(
        self,
        ip: str = "192.168.178.50",
        port: int = 6000,
        width: int = 64,
        height: int = 64
    ):
        """
        Initialize frame buffer.

        Args:
            ip: IP address of the Colorlight board
            port: UDP port for this panel
            width: Panel width in pixels
            height: Panel height in pixels
        """
        if not HAS_NUMPY:
            raise ImportError("NumPy is required for ColorlightFrameBuffer")

        self.ip = ip
        self.port = port
        self.width = width
        self.height = height
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.buffer = np.zeros((height, width, 3), dtype=np.uint8)

    def set_pixel(self, x: int, y: int, r: int, g: int, b: int) -> None:
        """Set pixel in buffer (8-bit RGB values)."""
        if 0 <= x < self.width and 0 <= y < self.height:
            self.buffer[y, x] = [r, g, b]

    def load_array(self, image_array) -> None:
        """
        Load a numpy array as the frame buffer.

        Args:
            image_array: NumPy array of shape (height, width, 3) with 8-bit RGB
        """
        self.buffer = np.clip(image_array, 0, 255).astype(np.uint8)

    def load_image(self, image_path: str) -> None:
        """
        Load an image file into the frame buffer.

        Requires PIL/Pillow.

        Args:
            image_path: Path to image file
        """
        from PIL import Image
        img = Image.open(image_path).resize((self.width, self.height)).convert("RGB")
        self.buffer = np.array(img)

    def send(self) -> None:
        """Send entire frame buffer to display."""
        for y in range(self.height):
            for x in range(self.width):
                r, g, b = self.buffer[y, x]
                # Convert 8-bit to 6-bit
                data = bytes([
                    y & 0x3F,
                    x & 0x3F,
                    (r >> 2) & 0x3F,
                    (g >> 2) & 0x3F,
                    (b >> 2) & 0x3F
                ])
                self.sock.sendto(data, (self.ip, self.port))

    def clear(self) -> None:
        """Clear the buffer to black."""
        self.buffer.fill(0)

    def close(self) -> None:
        """Close the UDP socket."""
        self.sock.close()


# Convenience functions
def create_display(ip: str = "192.168.178.50") -> ColorlightDisplay:
    """Create a display instance with default settings."""
    return ColorlightDisplay(ip=ip)


def create_framebuffer(ip: str = "192.168.178.50") -> ColorlightFrameBuffer:
    """Create a frame buffer instance with default settings."""
    return ColorlightFrameBuffer(ip=ip)
