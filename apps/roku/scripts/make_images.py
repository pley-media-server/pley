"""Draws the watched mark (images/watched.png: a disc with a check cut out
of it), the focus of buttons (images/button-focus.9.png, and where it was
as a button group lost it, images/button-footprint.9.png: nine-patch
pictures, a rounded box), images/spinner.png, which turns while a channel
tunes, images/round.png, which makes a person's photo round, and
images/globe.png, the globe that picks the language before signing in:
all white, for the screens to tint in the theme's colors (a Poster's
blendColor).

The Live TV guide (Roku's TimeGrid) takes pictures and no colors, so its
programs' boxes, its focus and what has aired come in every theme's
colors, named by them: images/guide/cell-<surface-2>.9.png,
focus-<accent>.9.png and past-<surface>.9.png ("cell-191D24.9.png"), for
each theme under each accent of packages/themes/themes.json, within
Roku's broadcast-safe range as the app draws them (broadcast_safe); run
this again when it changes (tests/themes.test.brs says when a picture is
missing). The icons and splash screens are pley's logo, rendered with the
other apps' icons by scripts/render-icons.mjs at the repository's root.
Standard library only.

    python scripts/make_images.py
"""

import json
import math
import os
import struct
import zlib

WHITE = (0xFF, 0xFF, 0xFF)
# What has aired, over the guide: the theme's surface at 60 %.
PAST_ALPHA = 0x99

# A button's box: its size inside the nine-patch's border, and its corners.
BOX = 32
RADIUS = 8

# The watched mark, drawn at this size on tiles (components/items/Tile.brs).
MARK_SIZE = 40

# The spinner a BusySpinner turns (components/screens/LivePlayerScreen).
SPINNER_SIZE = 64

# The mask of round pictures, a person's photo (components/screens/
# PersonScreen), drawn at this size.
ROUND_SIZE = 160

# The language's globe, drawn at this size beside its two letters
# (components/shared/LanguageGlobe).
GLOBE_SIZE = 28


def segment_distance(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def render_mark(size):
    """RGBA rows of the watched mark: a white disc, a check cut out of it
    (what is under the mark shows through)."""
    check = [(0.27, 0.52), (0.43, 0.68), (0.74, 0.36)]
    half_stroke = 0.065
    samples = 4
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            disc = ink = 0
            for sy in range(samples):
                for sx in range(samples):
                    x = (col + (sx + 0.5) / samples) / size
                    y = (row + (sy + 0.5) / samples) / size
                    if math.hypot(x - 0.5, y - 0.5) <= 0.5:
                        disc += 1
                        near = min(segment_distance(x, y, *check[i], *check[i + 1]) for i in range(len(check) - 1))
                        if near <= half_stroke:
                            ink += 1
            total = samples * samples
            line.extend(WHITE)
            line.append(round(255 * (disc - ink) / total))
        rows.append(bytes(line))
    return rows


def render_box(color, radius=RADIUS):
    """RGBA rows of a nine-patch (Roku's .9.png): a rounded box of color
    (with its alpha, if it has one), in a border of one pixel whose black
    marks say what stretches (top and left)."""
    alpha = color[3] if len(color) > 3 else 255
    color = color[:3]
    size = BOX + 2
    samples = 4
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            if row in (0, size - 1) or col in (0, size - 1):
                edge = max(radius, 1)
                marked = (row == 0 and edge < col < size - 1 - edge) or (col == 0 and edge < row < size - 1 - edge)
                line.extend((0, 0, 0, 255) if marked else (0, 0, 0, 0))
                continue
            inside = 0
            for sy in range(samples):
                for sx in range(samples):
                    x = col - 1 + (sx + 0.5) / samples
                    y = row - 1 + (sy + 0.5) / samples
                    dx = max(radius - x, 0, x - (BOX - radius))
                    dy = max(radius - y, 0, y - (BOX - radius))
                    if math.hypot(dx, dy) <= radius:
                        inside += 1
            line.extend(color)
            line.append(round(alpha * inside / (samples * samples)))
        rows.append(bytes(line))
    return rows


def render_spinner(size):
    """RGBA rows of a white ring, three quarters drawn, fading from its
    head to its tail."""
    samples = 4
    outer = 0.5
    inner = 0.38
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            cover = 0.0
            for sy in range(samples):
                for sx in range(samples):
                    x = (col + (sx + 0.5) / samples) / size - 0.5
                    y = (row + (sy + 0.5) / samples) / size - 0.5
                    r = math.hypot(x, y)
                    if inner <= r <= outer:
                        turn = (math.atan2(y, x) / (2 * math.pi)) % 1.0
                        if turn <= 0.75:
                            cover += 0.25 + 0.75 * turn / 0.75
            line.extend(WHITE)
            line.append(round(255 * cover / (samples * samples)))
        rows.append(bytes(line))
    return rows


def render_disc(size):
    """RGBA rows of a white disc as wide as the picture, its edge smoothed:
    the mask that makes a picture round (a MaskGroup's maskUri)."""
    samples = 4
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            inside = 0
            for sy in range(samples):
                for sx in range(samples):
                    x = (col + (sx + 0.5) / samples) / size - 0.5
                    y = (row + (sy + 0.5) / samples) / size - 0.5
                    if math.hypot(x, y) <= 0.5:
                        inside += 1
            line.extend((255, 255, 255))
            line.append(round(255 * inside / (samples * samples)))
        rows.append(bytes(line))
    return rows


def render_globe(size):
    """RGBA rows of a plain globe in white lines: its outline, a meridian
    and the equator, as the other TV apps draw it."""
    samples = 4
    radius = 0.44
    meridian = 0.2
    half_stroke = 0.045
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            ink = 0
            for sy in range(samples):
                for sx in range(samples):
                    x = (col + (sx + 0.5) / samples) / size - 0.5
                    y = (row + (sy + 0.5) / samples) / size - 0.5
                    r = math.hypot(x, y)
                    outline = abs(r - radius) <= half_stroke
                    ellipse = abs(math.hypot(x / meridian, y / radius) - 1) * meridian <= half_stroke
                    equator = abs(y) <= half_stroke and r <= radius
                    if outline or (r <= radius and ellipse) or equator:
                        ink += 1
            line.extend((255, 255, 255))
            line.append(round(255 * ink / (samples * samples)))
        rows.append(bytes(line))
    return rows


def write_png(path, width, height, rows):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + row for row in rows)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as out:
        out.write(png)


def theme_colors(spec):
    """Every theme's colors under every accent (none, the theme's own,
    first), as the web's colorOf has them: { token: "#rrggbb" }."""
    out = []
    for theme in spec["themes"]:
        for accent in [None] + spec["accents"]:
            colors = dict(theme["colors"])
            if accent:
                for token in theme["follows"]:
                    if token in accent[theme["kind"]]:
                        colors[token] = accent[theme["kind"]][token]
            out.append(colors)
    return out


WEIGHTS = (0.2126, 0.7152, 0.0722)


def luma(c):
    return WEIGHTS[0] * c[0] + WEIGHTS[1] * c[1] + WEIGHTS[2] * c[2]


DARKEST = luma((0x16, 0x16, 0x16))
LIGHTEST = luma((0xEB, 0xEB, 0xEB))


def broadcast_safe(color):
    """A color ("#rrggbb") within Roku's broadcast-safe range, as
    scripts/broadcast-safe.mjs moves it: each channel raised (or lowered)
    by the same amount, one at its end leaving the rest to the others,
    until the luma reaches the range's end."""
    c = [int(color[i:i + 2], 16) for i in (1, 3, 5)]
    start = luma(c)
    if DARKEST <= start <= LIGHTEST:
        return color.lower()
    up = start < DARKEST
    target = DARKEST if up else LIGHTEST
    for _ in range(3):
        free = [(v < 255) if up else (v > 0) for v in c]
        weight = sum(w for w, f in zip(WEIGHTS, free) if f)
        if weight == 0:
            break
        step = (target - luma(c)) / weight
        c = [min(255, max(0, v + step)) if f else v for v, f in zip(c, free)]
    # JavaScript's Math.round: halves go up.
    c = [int(math.floor(v + 0.5)) for v in c]
    while luma(c) < DARKEST:
        c = [min(255, v + 1) for v in c]
    while luma(c) > LIGHTEST:
        c = [max(0, v - 1) for v in c]
    return "#%02x%02x%02x" % tuple(c)


def rgb(color):
    color = broadcast_safe(color)
    return tuple(int(color[i:i + 2], 16) for i in (1, 3, 5))


def guide_pictures(folder, spec):
    """The guide's pictures in every theme's colors (see above); those of
    colors no theme has any more go."""
    guide = os.path.join(folder, "guide")
    os.makedirs(guide, exist_ok=True)
    wanted = {}
    for colors in theme_colors(spec):
        for kind, token, radius, alpha in (("cell", "surface-2", 6, ()), ("focus", "accent", 0, ()), ("past", "surface", 0, (PAST_ALPHA,))):
            color = broadcast_safe(colors[token])
            wanted["%s-%s.9.png" % (kind, color[1:].upper())] = (rgb(color) + alpha, radius)
    for name in os.listdir(guide):
        if name not in wanted:
            os.remove(os.path.join(guide, name))
    for name, (color, radius) in sorted(wanted.items()):
        write_png(os.path.join(guide, name), BOX + 2, BOX + 2, render_box(color, radius))
    print("images/guide:", len(wanted), "pictures")


def main():
    folder = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "images")
    os.makedirs(folder, exist_ok=True)
    write_png(os.path.join(folder, "watched.png"), MARK_SIZE, MARK_SIZE, render_mark(MARK_SIZE))
    print("images/watched.png", MARK_SIZE, "x", MARK_SIZE)
    for name in ("button-focus.9.png", "button-footprint.9.png"):
        write_png(os.path.join(folder, name), BOX + 2, BOX + 2, render_box(WHITE, RADIUS))
        print("images/" + name, BOX + 2, "x", BOX + 2)
    themes = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "packages", "themes", "themes.json")
    with open(themes, encoding="utf-8") as f:
        guide_pictures(folder, json.load(f))
    write_png(os.path.join(folder, "spinner.png"), SPINNER_SIZE, SPINNER_SIZE, render_spinner(SPINNER_SIZE))
    print("images/spinner.png", SPINNER_SIZE, "x", SPINNER_SIZE)
    write_png(os.path.join(folder, "round.png"), ROUND_SIZE, ROUND_SIZE, render_disc(ROUND_SIZE))
    print("images/round.png", ROUND_SIZE, "x", ROUND_SIZE)
    write_png(os.path.join(folder, "globe.png"), GLOBE_SIZE, GLOBE_SIZE, render_globe(GLOBE_SIZE))
    print("images/globe.png", GLOBE_SIZE, "x", GLOBE_SIZE)


if __name__ == "__main__":
    main()
