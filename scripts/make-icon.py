"""Encode the project's geometric eye mark as a Win32 DIB icon."""
import math
import struct
from pathlib import Path

size = 64
pixels = bytearray()
for y in reversed(range(size)):
    for x in range(size):
        px, py = x + 0.5, y + 0.5
        corner_x, corner_y = max(16 - px, px - 48, 0), max(16 - py, py - 48, 0)
        inside = corner_x * corner_x + corner_y * corner_y <= 16 * 16
        eye = 9 <= px <= 55 and abs(py - 32) <= 14 * (1 - ((px - 32) / 23) ** 2)
        radius = math.hypot(px - 32, py - 32)
        white = eye and (radius > 10 or radius < 5)
        pixels.extend((255, 255, 255, 255) if inside and white else (255, 117, 50, 255) if inside else (0, 0, 0, 0))
mask = bytes(((size + 31) // 32) * 4 * size)
dib = struct.pack('<IIIHHIIIIII', 40, size, size * 2, 1, 32, 0, len(pixels), 0, 0, 0, 0) + pixels + mask
ico = struct.pack('<HHH', 0, 1, 1) + struct.pack('<BBBBHHII', size, size, 0, 0, 1, 32, len(dib), 22) + dib
Path('assets/QuickPeek.ico').write_bytes(ico)
