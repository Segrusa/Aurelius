"""Render the Aurelius app icon: a dark green rounded square with the white
"A" from the in-app logo. No dependencies:
supersampled rasterizer, PNG via zlib, ICO and ICNS with PNG entries.

    python scripts/make-icon.py   →  clients/electron/assets/aurelius.ico (+ aurelius-512.png)
                                     clients/electron/assets/aurelius-dev.ico (dev builds:
                                     amber "DEV" strip, so the taskbar tells them apart)
                                     clients/electron/assets/aurelius.icns, aurelius-dev.icns
                                     (macOS app bundle, scripts/install-mac.sh: transparent
                                     margin as on Apple's icon grid)
                                     clients/mobile/ios/.../AppIcon.appiconset (full-bleed,
                                     opaque: iOS applies its own mask and rejects alpha)
                                     clients/mobile/android/.../mipmap-*/ic_launcher.png
"""
import os, struct, zlib

GREEN = (0x1F, 0x4A, 0x33)          # dark green background
WHITE = (0xFF, 0xFF, 0xFF)
# The logo's polygon in a 120×120 design box (same as the in-app logo).
GLYPH = [(60, 14), (104, 106), (82, 106), (60, 56), (38, 106), (16, 106)]
GLYPH_SCALE = 0.60                  # glyph box relative to the icon
RADIUS = 0.225                      # corner radius relative to the icon
MAC_MARGIN = 100 / 1024             # macOS icon grid: an 824 px body in 1024
AMBER = (0xB7, 0x79, 0x1F)          # same as the DEV badge in the title bar
DARK = (0x1A, 0x12, 0x06)
STRIP = 0.34                        # dev strip height relative to the icon
# 3×5 pixel letters for "DEV".
FONT = {
    "D": ["110", "101", "101", "101", "110"],
    "E": ["111", "100", "110", "100", "111"],
    "V": ["101", "101", "101", "101", "010"],
}

def in_text(x, y, size):
    """Inside the "DEV" letters, centred in the strip (only from 32 px up)."""
    if size < 32:
        return False
    strip_top = size * (1 - STRIP)
    unit = size * STRIP * 0.52 / 5
    width = unit * 11                           # 3 letters of 3 + 2 gaps of 1
    left = (size - width) / 2
    top = strip_top + (size * STRIP - unit * 5) / 2
    cx, cy = (x - left) / unit, (y - top) / unit
    if cx < 0 or cy < 0 or cx >= 11 or cy >= 5:
        return False
    col, row = int(cx), int(cy)
    letter, offset = divmod(col, 4)
    return offset < 3 and FONT["DEV"[letter]][row][offset] == "1"

def inside_polygon(x, y, poly):
    inside = False
    j = len(poly) - 1
    for i in range(len(poly)):
        xi, yi = poly[i]
        xj, yj = poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            inside = not inside
        j = i
    return inside

def inside_rounded(x, y, size, r):
    if x < 0 or y < 0 or x > size or y > size:
        return False
    cx = min(max(x, r), size - r)
    cy = min(max(y, r), size - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r

def render(size, dev=False, square=False, margin=0.0):
    ss = 8 if size <= 64 else 4
    # The icon body: the whole image, or smaller and centred with a transparent margin.
    edge = size * margin
    body = size - 2 * edge
    r = body * RADIUS
    g = body * (0.46 if dev else GLYPH_SCALE)
    ox = (body - g) / 2
    # Dev: the glyph moves up to make room for the strip.
    oy = body * 0.10 if dev else (body - g) / 2 + body * 0.01   # optical centring
    poly = [(ox + px * g / 120.0, oy + py * g / 120.0) for px, py in GLYPH]
    rows = []
    for y in range(size):
        row = bytearray([0])                    # PNG filter: none
        for x in range(size):
            acc = [0.0, 0.0, 0.0]
            hits = 0
            for sy in range(ss):
                for sx in range(ss):
                    fx = x + (sx + 0.5) / ss - edge
                    fy = y + (sy + 0.5) / ss - edge
                    if not square and not inside_rounded(fx, fy, body, r):
                        continue
                    hits += 1
                    if dev and fy >= body * (1 - STRIP):
                        color = DARK if in_text(fx, fy, body) else AMBER
                    elif inside_polygon(fx, fy, poly):
                        color = WHITE
                    else:
                        color = GREEN
                    for k in range(3):
                        acc[k] += color[k]
            if hits == 0:
                row += bytes(4)
                continue
            rgb = [round(acc[k] / hits) for k in range(3)]
            row += bytes(rgb if square else rgb + [round(hits / (ss * ss) * 255)])
        rows.append(bytes(row))
    return png(size, b"".join(rows), alpha=not square)

def png(size, raw, alpha=True):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6 if alpha else 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")

def ico(images):
    header = struct.pack("<HHH", 0, 1, len(images))
    entries, data = b"", b""
    offset = 6 + 16 * len(images)
    for size, blob in images:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(blob), offset)
        data += blob
        offset += len(blob)
    return header + entries + data

# ICNS entry types with PNG data, by pixel size (the @2x types share sizes).
ICNS_TYPES = [(b"icp4", 16), (b"icp5", 32), (b"ic11", 32), (b"ic12", 64), (b"ic07", 128),
              (b"ic13", 256), (b"ic08", 256), (b"ic14", 512), (b"ic09", 512), (b"ic10", 1024)]

def icns(render_size):
    rendered, data = {}, b""
    for kind, size in ICNS_TYPES:
        if size not in rendered:
            rendered[size] = render_size(size)
        data += kind + struct.pack(">I", 8 + len(rendered[size])) + rendered[size]
    return b"icns" + struct.pack(">I", 8 + len(data)) + data

if __name__ == "__main__":
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out = os.path.join(root, "clients", "electron", "assets")
    os.makedirs(out, exist_ok=True)
    sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
    images = [(s, render(s)) for s in sizes]
    with open(os.path.join(out, "aurelius.ico"), "wb") as f:
        f.write(ico(images))
    with open(os.path.join(out, "aurelius-512.png"), "wb") as f:
        f.write(render(512))
    with open(os.path.join(out, "aurelius-dev.ico"), "wb") as f:
        f.write(ico([(s, render(s, dev=True)) for s in sizes]))
    print("written", os.path.join(out, "aurelius.ico"), "and aurelius-dev.ico")
    with open(os.path.join(out, "aurelius.icns"), "wb") as f:
        f.write(icns(lambda s: render(s, margin=MAC_MARGIN)))
    with open(os.path.join(out, "aurelius-dev.icns"), "wb") as f:
        f.write(icns(lambda s: render(s, dev=True, margin=MAC_MARGIN)))
    print("written", os.path.join(out, "aurelius.icns"), "and aurelius-dev.icns")

    mobile = os.path.join(root, "clients", "mobile")
    ios = os.path.join(mobile, "ios", "Runner", "Assets.xcassets", "AppIcon.appiconset")
    ios_icons = [(20, 1), (20, 2), (20, 3), (29, 1), (29, 2), (29, 3), (40, 1), (40, 2),
                 (40, 3), (60, 2), (60, 3), (76, 1), (76, 2), (83.5, 2), (1024, 1)]
    rendered = {}
    for pt, scale in ios_icons:
        px = round(pt * scale)
        if px not in rendered:
            rendered[px] = render(px, square=True)
        name = "Icon-App-%sx%s@%dx.png" % ("%g" % pt, "%g" % pt, scale)
        with open(os.path.join(ios, name), "wb") as f:
            f.write(rendered[px])
    print("written", ios)

    res = os.path.join(mobile, "android", "app", "src", "main", "res")
    for density, px in [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)]:
        with open(os.path.join(res, "mipmap-" + density, "ic_launcher.png"), "wb") as f:
            f.write(render(px))
    print("written", res)
