# Writes the setup logo: logo.png (256 px) and logo.ico (16 to 256 px).
#
#   uv run make-logo.py logo.png logo.ico
#
# The Rivet mark is two interlocking fastener heads. The open cuts point at each other: a
# browser session stays attached to the user's browser while Rivet supplies its own surface.
# It is drawn on a 96 unit grid and supersampled, so every size is drawn from shapes rather
# than scaled.
import struct, sys, zlib

GRID = 96.0
TILE_TOP = (0x10, 0x14, 0x31)
TILE_BOTTOM = (0x27, 0x16, 0x47)
COBALT = (0x41, 0x7C, 0xFF)
VIOLET = (0x91, 0x58, 0xFF)
CORAL = (0xFF, 0x6B, 0x73)
INK = (0x10, 0x14, 0x31)


def rounded(x, y, x0, y0, x1, y1, r):
    """Whether (x, y) is inside the rectangle x0..x1, y0..y1 with corners of radius r."""
    if x < x0 or x >= x1 or y < y0 or y >= y1:
        return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def circle(x, y, cx, cy, radius):
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius


def blend(a, b, t):
    t = min(1.0, max(0.0, t))
    return tuple(round(start + (end - start) * t) for start, end in zip(a, b))


def colour(x, y):
    """The colour at grid point (x, y), or None outside the tile."""
    if not rounded(x, y, 0, 0, 96, 96, 22):
        return None

    c = blend(TILE_TOP, TILE_BOTTOM, (x + y) / (GRID * 2))
    # Two open rings form the fastening point. The lower ring is painted first, which gives
    # the icon a clear reading order even at 16 px.
    if circle(x, y, 61, 61, 28) and not circle(x, y, 61, 61, 13):
        c = blend(VIOLET, CORAL, (x + y - 64) / 64)
    if circle(x, y, 37, 37, 28) and not circle(x, y, 37, 37, 13):
        c = blend(COBALT, VIOLET, (x + y) / 80)

    # Facing diagonal cuts keep the two rings separate and recognizable as a connection.
    if 41 < x < 61 and 41 < y < 61 and abs(x - y) < 8:
        c = INK
    if circle(x, y, 37, 37, 12) or circle(x, y, 61, 61, 12):
        c = INK
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
