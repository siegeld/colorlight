#!/usr/bin/env python3
# To flash: ecpprog -o 1536k img_data.bin

import png


def _get_indexed_image_array():
    r = png.Reader(file=open("./demo_img_indexed.png", "rb"))
    img = r.read()
    assert img[0] == 64
    assert img[1] == 64
    pixels = list(img[2])
    out_array = []
    # Get image data
    for arr in pixels:
        for i in range(64):
            out_array.append(arr[i])
    # Get palette data
    # rgbrgbrgb
    palette = []
    # Probably rgb?
    png_palette = img[3]["palette"]
    for a in png_palette:
        palette.append(a[0] | a[1] << 8 | a[2] << 16)
    return (out_array, img[0], img[1], palette)


def write_32bit(filehandler, data):
    byte_arr = [data & 0xFF, (data >> 8) & 0xFF, (data >> 16) & 0xFF, (data >> 24) & 0xFF]
    f.write(bytearray(byte_arr))

img = _get_indexed_image_array()

f = open("img_data_indexed.bin", "wb")
width = img[1]
write_32bit(f, 1 << 31 | (width & 0xFFFF))
write_32bit(f, len(img[0]))
write_32bit(f, 0xD1581A40)
write_32bit(f, 0xDA5A0001)
for _ in range(240//4):
    write_32bit(f, 0x0)
for data in img[0]:
    f.write(bytearray([data]))
for color in img[3]:
    write_32bit(f, color)
f.close()
