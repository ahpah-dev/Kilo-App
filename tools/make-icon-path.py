"""Render the compact in-app K path (and alternatives) for visual checking."""
import importlib.util
import sys

from PIL import Image, ImageDraw

spec = importlib.util.spec_from_file_location("mk", "tools/make-logo.py")
mk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mk)

SIZE = 1024
img = mk.compose_icon(SIZE, background=True, letter="K").convert("RGBA")
px = img.load()


def is_ink(x, y):
    if x < 0 or y < 0 or x >= SIZE or y >= SIZE:
        return False
    r, g, b, a = px[x, y]
    return a > 128 and r > 200 and g > 200 and b > 200


N = [(-1, 0), (-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1)]
ink = {(x, y) for y in range(SIZE) for x in range(SIZE) if is_ink(x, y)}
start = min(ink, key=lambda p: (p[1], p[0]))

cont = [start]
cur, bt, steps = start, 0, 0
while steps < 8 * len(ink) + 16:
    steps += 1
    adv = False
    for k in range(8):
        idx = (bt + 1 + k) % 8
        dx, dy = N[idx]
        nxt = (cur[0] + dx, cur[1] + dy)
        if nxt in ink:
            cont.append(nxt)
            bt = (idx + 4) % 8
            cur = nxt
            adv = True
            break
    if not adv or cur == start:
        break


def rdp(p, eps):
    if len(p) < 3:
        return list(p)
    (x1, y1), (x2, y2) = p[0], p[-1]
    dx, dy = x2 - x1, y2 - y1
    norm = (dx * dx + dy * dy) ** 0.5
    worst, idx = 0.0, 0
    for i in range(1, len(p) - 1):
        x0, y0 = p[i]
        d = ((x0 - x1) ** 2 + (y0 - y1) ** 2) ** 0.5 if norm == 0 \
            else abs(dy * x0 - dx * y0 + x2 * y1 - y2 * x1) / norm
        if d > worst:
            worst, idx = d, i
    if worst <= eps:
        return [p[0], p[-1]]
    return rdp(p[:idx + 1], eps)[:-1] + rdp(p[idx:], eps)


pts = [(x + 0.5, y + 0.5) for (x, y) in cont]
if pts[0] == pts[-1]:
    pts = pts[:-1]

scale = 24.0 / SIZE
for tol in (2.0,):
    s = rdp(pts + [pts[0]], tol)
    if s[0] == s[-1]:
        s = s[:-1]
    poly = [(round(x * scale, 2), round(y * scale, 2)) for x, y in s]
    out = []
    for q in poly:
        if not out or out[-1] != q:
            out.append(q)
    d = "M" + " L".join(f"{x},{y}" for x, y in out) + "Z"
    print(f"tol={tol} vertices={len(out)}")
    print(f"ICON_PATH={d}")
    # Render the icon at 24u then at the sizes the app actually uses.
    sheet = Image.new("RGBA", (24 * 40, 24 * 40), (245, 246, 250, 255))
    for i, px_size in enumerate((16, 24, 32)):
        canvas = Image.new("RGBA", (px_size * 8, px_size * 8), (0, 0, 0, 0))
        dr = ImageDraw.Draw(canvas)
        f = px_size * 8 / 24.0
        dr.polygon([(x * f, y * f) for x, y in out], fill=(99, 102, 241, 255))
        sheet.alpha_composite(canvas.resize((px_size * 8, px_size * 8), Image.LANCZOS),
                              (40 + i * (px_size * 8 + 40), 100))
    sheet.save("tools/out/icon-k-compact.png")
    print("wrote tools/out/icon-k-compact.png")
