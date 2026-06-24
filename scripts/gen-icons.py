#!/usr/bin/env python3
"""Generate placeholder icons for github-notifier-ws Tauri build.

Run from repo root: python scripts/gen-icons.py
No external dependencies required.
"""
import os
import struct
import zlib

ICONS_DIR = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")

# Base color (GitHub blue)
COLOR = (3, 102, 214)

# State icon colors
STATES = {
    "idle":   (100, 110, 122),   # gray
    "unread": (45,  164,  78),   # GitHub green
    "error":  (209,  36,  47),   # red
    "paused": (87,  96, 106),    # dim gray
}


def make_png(width: int, height: int, r: int, g: int, b: int) -> bytes:
    def chunk(name: bytes, data: bytes) -> bytes:
        body = name + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    # Color type 6 = RGBA (required by Tauri)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    raw = b"".join(b"\x00" + bytes([r, g, b, 255] * width) for _ in range(height))
    compressed = zlib.compress(raw, 9)

    data = b"\x89PNG\r\n\x1a\n"
    data += chunk(b"IHDR", ihdr)
    data += chunk(b"IDAT", compressed)
    data += chunk(b"IEND", b"")
    return data


def make_ico(sizes, r, g, b) -> bytes:
    """Minimal ICO containing one or more PNG images."""
    images = [(s, make_png(s, s, r, g, b)) for s in sizes]

    header_size = 6
    dir_entry_size = 16
    offset = header_size + dir_entry_size * len(images)

    ico = struct.pack("<HHH", 0, 1, len(images))
    for size, data in images:
        ico_size = size if size < 256 else 0
        ico += struct.pack("<BBBBHHII", ico_size, ico_size, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    for _, data in images:
        ico += data
    return ico


def write(path: str, data: bytes):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
    print(f"  wrote {path}")


if __name__ == "__main__":
    r, g, b = COLOR
    print("Generating base icons...")

    write(os.path.join(ICONS_DIR, "32x32.png"), make_png(32, 32, r, g, b))
    write(os.path.join(ICONS_DIR, "128x128.png"), make_png(128, 128, r, g, b))
    write(os.path.join(ICONS_DIR, "128x128@2x.png"), make_png(256, 256, r, g, b))
    write(os.path.join(ICONS_DIR, "icon.icns"), make_png(512, 512, r, g, b))  # stub
    write(os.path.join(ICONS_DIR, "icon.ico"), make_ico([16, 32, 48, 256], r, g, b))
    write(os.path.join(ICONS_DIR, "icon.png"), make_png(512, 512, r, g, b))

    print("Generating state icons...")
    for name, (sr, sg, sb) in STATES.items():
        write(os.path.join(ICONS_DIR, f"state_{name}.png"), make_png(32, 32, sr, sg, sb))

    print("Done.")
