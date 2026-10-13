"""Generate the Kilo App logo and the full Tauri icon set.

The brand mark is the isometric cube used by the in-app logo (src/components/Icon.vue)
and the web favicon: a hexagonal outline with the three interior edges meeting at
the centre, plus a centre dot.

Usage:
    python make-logo.py            # writes all icons + assets/logo.svg + preview
"""
from __future__ import annotations

import os
from PIL import Image, ImageDraw, ImageFilter

ICON_DIR = os.path.join("src-tauri", "icons")
ASSET_DIR = "assets"
# Dev-only output; kept out of assets/ so it is not committed as a product asset.
PREVIEW_DIR = os.path.join("tools", "out")

# Brand colours (indigo accent from src/styles/main.css: --accent #6366f1).
ACCENT_LIGHT = (129, 140, 248)   # #818cf8
ACCENT_BASE = (99, 102, 241)     # #6366f1
ACCENT_DEEP = (67, 56, 202)      # #4338ca

# Cube glyph geometry, in the 24x24 coordinate space of Icon.vue/favicon.svg.
S = 4                            # supersampling factor
CANVAS = 1024                    # master canvas size
CUBE_BOX = 660                   # cube glyph bounding box inside the canvas


def lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def mix(c1, c2, t: float):
    return tuple(int(round(lerp(c1[i], c2[i], t))) for i in range(3))


def gradient(size: int, c1, c2, diagonal: bool = True) -> Image.Image:
    """Linear gradient from c1 (top-left) to c2 (bottom-right)."""
    img = Image.new("RGB", (size, size))
    px = img.load()
    for y in range(size):
        for x in range(size):
            t = (x + y) / (2 * (size - 1)) if diagonal else y / (size - 1)
            px[x, y] = mix(c1, c2, t)
    return img


def rounded_mask(size: int, radius: int, ss: int = 4) -> Image.Image:
    """Antialiased rounded-square alpha mask."""
    m = Image.new("L", (size * ss, size * ss), 0)
    ImageDraw.Draw(m).rounded_rectangle(
        (0, 0, size * ss - 1, size * ss - 1), radius=radius * ss, fill=255
    )
    return m.resize((size, size), Image.LANCZOS)


def cube_points(box: int) -> dict:
    """Scale the 24x24 cube path into a `box`-sized square."""

    def p(x: float, y: float):
        return (x * box / 24.0, y * box / 24.0)

    return {
        "v_top": p(12, 2),
        "v_ur": p(21, 7),
        "v_lr": p(21, 17),
        "v_bot": p(12, 22),
        "v_ll": p(3, 17),
        "v_ul": p(3, 7),
        "v_mid": p(12, 12),
        "v_dot": p(12, 12),
        "dot_r": 2.0 * box / 24.0,
    }


def draw_cube(img: Image.Image, box: int, offset: tuple, colour, stroke: int, ss: int = S):
    """Draw the cube wireframe (hexagon outline + 3 interior edges + centre dot)."""
    layer = Image.new("RGBA", (img.width * ss, img.height * ss), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    pts = cube_points(box)
    ox, oy = offset[0] * ss, offset[1] * ss
    w = max(1, round(stroke * ss))

    def at(key):
        return (pts[key][0] + ox, pts[key][1] + oy)

    hexagon = [at("v_top"), at("v_ur"), at("v_lr"), at("v_bot"), at("v_ll"), at("v_ul")]
    d.line(hexagon + [hexagon[0]], fill=colour, width=w, joint="curve")
    for key in ("v_ul", "v_ur", "v_bot"):
        d.line([at("v_mid"), at(key)], fill=colour, width=w, joint="curve")

    # Centre dot, drawn as a filled circle to match the in-app logo.
    cx, cy = at("v_dot")
    r = pts["dot_r"] * ss
    d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=colour)

    layer = layer.resize((img.width, img.height), Image.LANCZOS)
    img.alpha_composite(layer)


def compose_icon(size: int, background: bool = True, ss: int = S) -> Image.Image:
    """Render the icon for the given `size` with `ss`x supersampling."""
    big = size * ss
    if background:
        base = gradient(big, ACCENT_LIGHT, ACCENT_DEEP).convert("RGBA")
        base.putalpha(rounded_mask(big, radius=int(big * 0.22), ss=1))
        # Very soft top sheen: blurred mask = no hard ellipse edge.
        sheen = Image.new("L", (big, big), 0)
        ImageDraw.Draw(sheen).ellipse(
            (-big // 2, -int(big * 0.95), big + big // 2, int(big * 0.55)), fill=30
        )
        sheen = sheen.filter(ImageFilter.GaussianBlur(big * 0.10))
        sheen_layer = Image.new("RGBA", (big, big), (255, 255, 255, 0))
        sheen_layer.putalpha(sheen)
        base.alpha_composite(sheen_layer)
    else:
        base = Image.new("RGBA", (big, big), (0, 0, 0, 0))

    glyph_box = big * CUBE_BOX / CANVAS
    offset = ((big - glyph_box) / 2, (big - glyph_box) / 2)
    colour = (255, 255, 255, 255) if background else mix(ACCENT_LIGHT, ACCENT_DEEP, 0.45) + (255,)
    # Stroke scales with the glyph so every export keeps the same optical weight.
    draw_cube(base, glyph_box, offset, colour, stroke=max(1, round(big * 0.052)), ss=1)

    return base.resize((size, size), Image.LANCZOS)


def make_icon(size: int = CANVAS, background: bool = True) -> Image.Image:
    return compose_icon(size, background)


def save_png(img: Image.Image, path: str, size: int):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    img.resize((size, size), Image.LANCZOS).save(path, "PNG")
    print(f"  {path}  {size}x{size}")


def main():
    os.makedirs(ICON_DIR, exist_ok=True)
    os.makedirs(ASSET_DIR, exist_ok=True)
    os.makedirs(PREVIEW_DIR, exist_ok=True)

    print("rendering master icon...")
    master = compose_icon(CANVAS, background=True)
    master.save(os.path.join(ASSET_DIR, "logo-1024.png"), "PNG")
    print(f"  {ASSET_DIR}/logo-1024.png  1024x1024")

    print("writing PNG sizes...")
    save_png(master, os.path.join(ICON_DIR, "32x32.png"), 32)
    save_png(master, os.path.join(ICON_DIR, "128x128.png"), 128)
    save_png(master, os.path.join(ICON_DIR, "128x128@2x.png"), 256)
    save_png(master, os.path.join(ICON_DIR, "icon.png"), 512)
    for n in (30, 44, 71, 89, 107, 142, 150, 284, 310):
        save_png(master, os.path.join(ICON_DIR, f"Square{n}x{n}Logo.png"), n)
    save_png(master, os.path.join(ICON_DIR, "StoreLogo.png"), 50)

    print("writing icon.ico (multi-size, for taskbar/desktop/explorer)...")
    ico_path = os.path.join(ICON_DIR, "icon.ico")
    master.save(ico_path, "ICO",
                sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print(f"  {ico_path}")

    print("writing icon.icns (macOS)...")
    icns_path = os.path.join(ICON_DIR, "icon.icns")
    try:
        master.save(icns_path, "ICNS")
        print(f"  {icns_path}")
    except Exception as exc:  # Pillow needs specific sizes for ICNS
        print(f"  ICNS skipped: {exc}")

    print("writing transparent cube glyph (for the website)...")
    glyph = make_icon(512, background=False)
    glyph.save(os.path.join(ASSET_DIR, "logo-glyph-512.png"), "PNG")
    print(f"  {ASSET_DIR}/logo-glyph-512.png  512x512")

    # A quick visual sheet so the result can be eyeballed (not part of the app).
    sizes = (256, 128, 64, 48, 32, 16)
    pad = 24
    width = pad + sum(s + pad for s in sizes)
    sheet = Image.new("RGBA", (width, 256 + pad * 2), (245, 246, 250, 255))
    x = pad
    for size in sizes:
        sheet.alpha_composite(master.resize((size, size), Image.LANCZOS),
                              (x, pad + (256 - size) // 2))
        x += size + pad
    sheet.save(os.path.join(PREVIEW_DIR, "logo-preview.png"), "PNG")
    print(f"  {PREVIEW_DIR}/logo-preview.png (visual check)")


if __name__ == "__main__":
    main()
