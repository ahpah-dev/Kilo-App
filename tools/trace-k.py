"""Emit SVG paths for the "K" brand mark, traced from the rendered master.

Tracing the rasterised icon (rather than shipping a <text> element) means the
favicon and site logo need no font installed on the viewer's machine, and the
shape is guaranteed to match the app icon exactly.

A "K" is concave (the arm and leg meet the stem at a notch), so a per-scanline
run trace is not sufficient. This uses Moore-neighbour boundary tracing to
follow the outer contour in order, then thins it with Ramer-Douglas-Peucker.
"""
import importlib.util
import sys

spec = importlib.util.spec_from_file_location("mk", "tools/make-logo.py")
mk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mk)

SIZE = 1024
UPSCALE = 2          # trace at 2x for sub-pixel edge precision
TOLERANCE_PX = 0.4   # RDP tolerance, in master pixels

img = mk.compose_icon(SIZE, background=True, letter="K").convert("RGBA")
px = img.load()
W = H = SIZE


def is_ink(x, y):
    if x < 0 or y < 0 or x >= W or y >= H:
        return False
    r, g, b, a = px[x, y]
    return a > 128 and r > 200 and g > 200 and b > 200


# Moore-neighbour offsets, clockwise starting from "west".
NEIGHBOURS = [(-1, 0), (-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1)]


def trace_contour(start):
    """Moore-neighbour boundary tracing, returning the outer contour in order.

    `start` must be a boundary pixel entered from the west (i.e. no ink to its
    west), which is true of the topmost-leftmost ink pixel.
    """
    ink = {(x, y) for y in range(H) for x in range(W) if is_ink(x, y)}

    contour = [start]
    current = start
    # We arrived at `start` travelling east, so begin searching from the pixel
    # just after west (i.e. north-west is index 1 in the clockwise list).
    backtrack = 0  # "west"
    steps = 0
    limit = 8 * len(ink) + 16

    while steps < limit:
        steps += 1
        advanced = False
        for k in range(8):
            idx = (backtrack + 1 + k) % 8
            dx, dy = NEIGHBOURS[idx]
            nxt = (current[0] + dx, current[1] + dy)
            if nxt in ink:
                contour.append(nxt)
                # Next search starts from the neighbour we came from.
                backtrack = (idx + 4) % 8
                current = nxt
                advanced = True
                break
        if not advanced:
            break
        # Closed once we return to the start pixel.
        if current == start:
            break
    return contour, ink


# Find the first boundary pixel: topmost row, leftmost ink.
start = None
for y in range(H):
    for x in range(W):
        if is_ink(x, y):
            start = (x, y)
            break
    if start:
        break

if start is None:
    raise SystemExit("no ink found - did the letter render?")

contour, _ink = trace_contour(start)
sys.stderr.write(f"traced {len(contour)} boundary pixels from {start}\n")


def rdp(points, eps):
    if len(points) < 3:
        return list(points)
    (x1, y1), (x2, y2) = points[0], points[-1]
    dx, dy = x2 - x1, y2 - y1
    norm = (dx * dx + dy * dy) ** 0.5
    worst, index = 0.0, 0
    for i in range(1, len(points) - 1):
        x0, y0 = points[i]
        dist = ((x0 - x1) ** 2 + (y0 - y1) ** 2) ** 0.5 if norm == 0 \
            else abs(dy * x0 - dx * y0 + x2 * y1 - y2 * x1) / norm
        if dist > worst:
            worst, index = dist, i
    if worst <= eps:
        return [points[0], points[-1]]
    return rdp(points[:index + 1], eps)[:-1] + rdp(points[index:], eps)


# Work in float pixel space, then offset by half a pixel to land on pixel centres.
pts = [(x + 0.5, y + 0.5) for (x, y) in contour]
# RDP needs the ring open; drop the duplicated closing point.
if pts[0] == pts[-1]:
    pts = pts[:-1]
simple = rdp(pts + [pts[0]], TOLERANCE_PX)
if simple[0] == simple[-1]:
    simple = simple[:-1]

xs = [p[0] for p in simple]
ys = [p[1] for p in simple]
sys.stderr.write(f"vertices: {len(simple)}\n")
sys.stderr.write(f"ink bbox: x {min(xs):.1f}..{max(xs):.1f}  y {min(ys):.1f}..{max(ys):.1f}\n")
sys.stderr.write(f"ink size: {max(xs)-min(xs):.1f} x {max(ys)-min(ys):.1f}  "
                 f"cap/tile = {(max(ys)-min(ys))/SIZE:.4f}\n")


def to_path(target_units: int, precision: int):
    """Scale the contour into `target_units` space, re-centred horizontally."""
    scale = target_units / SIZE
    pts = [(round(x * scale, precision), round(y * scale, precision)) for x, y in simple]
    cx = target_units / 2
    glyph_cx = (min(p[0] for p in pts) + max(p[0] for p in pts)) / 2
    dx = round(cx - glyph_cx, precision)
    pts = [(round(x + dx, precision), y) for x, y in pts]
    # Drop consecutive duplicates created by rounding.
    out = []
    for p in pts:
        if not out or out[-1] != p:
            out.append(p)
    if len(out) > 1 and out[0] == out[-1]:
        out.pop()
    path = "M" + " L".join(f"{x},{y}" for x, y in out) + "Z"
    return path, len(out)


path24, n24 = to_path(24, 1)
path1024, n1024 = to_path(1024, 0)
sys.stderr.write(f"paths: {n24} / {n1024} vertices, {len(path24)} / {len(path1024)} chars\n")

TILE = 1024
BRAND = """  <defs>
    <linearGradient id="brand" gradientUnits="userSpaceOnUse" x1="512" y1="512" x2="1582.7" y2="1169.7">
      <stop offset="0" stop-color="#818cf8"/>
      <stop offset="1" stop-color="#4338ca"/>
    </linearGradient>
    <clipPath id="tile">
      <rect width="1024" height="1024" rx="225"/>
    </clipPath>
  </defs>
  <g clip-path="url(#tile)">
    <rect width="1024" height="1024" fill="url(#brand)"/>
    <ellipse cx="512" cy="-110" rx="800" ry="480" fill="#ffffff" opacity="0.12"/>
  </g>
"""

tile_svg = (
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" '
    'role="img" aria-label="Kilo App">\n'
    "  <title>Kilo App</title>\n"
    f"{BRAND}"
    f'  <path d="{path1024}" fill="#ffffff"/>\n'
    "</svg>\n"
)

for rel in ("public/favicon.svg", "docs/favicon.svg", "public/logo.svg", "docs/logo.svg"):
    with open(rel, "w", encoding="utf-8") as fh:
        fh.write(tile_svg)
    sys.stderr.write(f"  wrote {rel}\n")

# Glyph-only mark with a tight viewBox, for use on light backgrounds.
gx0, gy0 = min(x for x, _ in simple), min(y for _, y in simple)
gx1, gy1 = max(x for x, _ in simple), max(y for _, y in simple)
pad = 6
vx, vy = round(gx0 - pad, 1), round(gy0 - pad, 1)
vw, vh = round(gx1 - gx0 + 1 + 2 * pad, 1), round(gy1 - gy0 + 1 + 2 * pad, 1)
glyph_svg = (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vx} {vy} {vw} {vh}" '
    'role="img" aria-label="Kilo">\n'
    "  <title>Kilo</title>\n"
    "  <defs>\n"
    f'    <linearGradient id="brand" gradientUnits="userSpaceOnUse" '
    f'x1="{vx}" y1="{vy}" x2="{round(vx+vw,1)}" y2="{round(vy+vh,1)}">\n'
    '      <stop offset="0" stop-color="#818cf8"/>\n'
    '      <stop offset="1" stop-color="#4338ca"/>\n'
    "    </linearGradient>\n"
    "  </defs>\n"
    f'  <path d="{path1024}" fill="url(#brand)"/>\n'
    "</svg>\n"
)
with open("assets/logo-glyph.svg", "w", encoding="utf-8") as fh:
    fh.write(glyph_svg)
sys.stderr.write("  wrote assets/logo-glyph.svg\n")
