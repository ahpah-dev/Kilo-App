"""Trim the OS window-border artifacts from a captured app screenshot.

The captured window screenshot includes a few pixels of the frame/border on each
edge. The app surface itself has a flat background, so uniform edge lines are
detected and removed.
"""
from PIL import Image
import sys

SRC = sys.argv[1] if len(sys.argv) > 1 else r"C:\Users\David\.dsh\attachments\v1\objects\1e\1ed480ada702489b7aec70c0802ab27479616dd35def83c5dfe78ba905f30462"
DST = "docs/screenshot.png"

img = Image.open(SRC).convert("RGB")
w, h = img.size
px = img.load()

print(f"source: {w}x{h}")


def row_is_uniform(y, tol=6):
    first = px[0, y]
    for x in range(0, w, 3):
        c = px[x, y]
        if max(abs(c[i] - first[i]) for i in range(3)) > tol:
            return False
    return True


def col_is_uniform(x, tol=6):
    first = px[x, 0]
    for y in range(0, h, 3):
        c = px[x, y]
        if max(abs(c[i] - first[i]) for i in range(3)) > tol:
            return False
    return True


top = 0
while top < h // 8 and row_is_uniform(top):
    top += 1
bottom = h - 1
while bottom > h * 7 // 8 and row_is_uniform(bottom):
    bottom -= 1
left = 0
while left < w // 8 and col_is_uniform(left):
    left += 1
right = w - 1
while right > w * 7 // 8 and col_is_uniform(right):
    right -= 1

# Keep a 1px inset so no border remnant survives.
crop = (left + 1, top + 1, right, bottom)
out = img.crop(crop)
print(f"trimmed to: {out.size}  (crop={crop})")
out.save(DST, "PNG", optimize=True)
print(f"wrote {DST}")
