# Writes the setup logo: logo.png (256 px) and logo.ico (16 to 256 px).
#
#   uv run make-logo.py logo.png logo.ico
#
# The Rivet mark is a centered rivet head: an ink collar, a paper recess and a claret pin.
# The three concentric forms keep their reading order at taskbar size and share the browser
# surface's paper, ink and claret palette.
# It is drawn on a 96 unit grid and supersampled, so every size is drawn from shapes rather
# than scaled.
import struct, sys, zlib

GRID = 96.0
PAPER = (0xF2, 0xEE, 0xE5)
INK = (0x1F, 0x20, 0x24)
ACCENT = (0x7C, 0x2D, 0x4F)


def rounded(x, y, x0, y0, x1, y1, r):
    """Whether (x, y) is inside the rectangle x0..x1, y0..y1 with corners of radius r."""
    if x < x0 or x >= x1 or y < y0 or y >= y1:
        return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def circle(x, y, cx, cy, radius):
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius


def colour(x, y):
    """The colour at grid point (x, y), or None outside the tile."""
    if not rounded(x, y, 0, 0, 96, 96, 22):
        return None
    c = PAPER
    # A compact ring and centred pin make the fastener literal without becoming a letterform.
    if circle(x, y, 48, 48, 31) and not circle(x, y, 48, 48, 19):
        c = INK
    if circle(x, y, 48, 48, 9):
        c = ACCENT
    return c


def render(size, samples=4):
    rows = []
    step = GRID / size / samples
    for py in range(size):
        row = bytearray()
        for px in range(size):
            r = g = b = n = 0
            for sy in range(samples):
                for sx in range(samples):
                    c = colour((px * samples + sx + 0.5) * step, (py * samples + sy + 0.5) * step)
                    if c:
                        r += c[0]; g += c[1]; b += c[2]; n += 1
            if n:
                row += bytes((round(r / n), round(g / n), round(b / n), round(255 * n / samples ** 2)))
            else:
                row += bytes(4)
        rows.append(bytes(row))
    return rows


def png(size):
    raw = b"".join(b"\x00" + row for row in render(size))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


open(sys.argv[1], "wb").write(png(256))

# The icon: one PNG entry per size (Windows Vista and later read PNG entries).
if len(sys.argv) > 2:
    sizes = [16, 20, 24, 32, 40, 48, 64, 256]
    images = [png(s) for s in sizes]
    offset = 6 + 16 * len(sizes)
    header = struct.pack("<HHH", 0, 1, len(sizes))
    entries = b""
    for s, data in zip(sizes, images):
        entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    open(sys.argv[2], "wb").write(header + entries + b"".join(images))
