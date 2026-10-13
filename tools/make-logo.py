"""Generate the Kilo App logo and the full Tauri icon set.

The brand mark is a white "K" on the indigo gradient tile used throughout the
app: a diagonal gradient from --accent-hover to the deep indigo, on a rounded
square with a soft top sheen.

Usage:
    python make-logo.py            # writes all icons + assets/ + preview sheet
"""
from __future__ import annotations

import argparse
import os
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ICON_DIR = os.path.join("src-tauri", "icons")
ASSET_DIR = "assets"
# Dev-only output; kept out of assets/ so it is not committed as a product asset.
PREVIEW_DIR = os.path.join("tools", "out")

# Brand colours (indigo accent from src/styles/main.css: --accent #6366f1).
ACCENT_LIGHT = (129, 140, 248)   # #818cf8
ACCENT_BASE = (99, 102, 241)     # #6366f1
ACCENT_DEEP = (67, 56, 202)      # #4338ca

S = 4                            # supersampling factor
CANVAS = 1024                    # master canvas size
# Cap height of the letter as a fraction of the canvas; tuned so the K has
# balanced margins and stays legible at 16px.
LETTER_CAP = 0.62
# Horizontal optical adjustment, as a fraction of the canvas width. Left at 0:
# centring the glyph's ink box already yields balanced margins, and any nudge
# here would also have to be mirrored in tools/trace-k.py to stay consistent.
LETTER_NUDGE_X = 0.0

# Bold sans faces, in preference order. Segoe UI Bold matches the app's own
# font stack (--font-sans in src/styles/main.css).
FONT_CANDIDATES = (
    r"C:\Windows\Fonts\segoeuib.ttf",     # Segoe UI Bold
    r"C:\Windows\Fonts\seguisb.ttf",      # Segoe UI Semibold
    r"C:\Windows\Fonts\arialbd.ttf",      # Arial Bold
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
)


def find_font() -> str:
    for path in FONT_CANDIDATES:
        if os.path.exists(path):
            return path
    raise SystemExit(
        "No bold sans font found. Add one to FONT_CANDIDATES in tools/make-logo.py."
    )


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


def render_letter(size: int, letter: str, cap_ratio: float = LETTER_CAP) -> Image.Image:
    """White `letter` on a transparent canvas, auto-fitted and centred.

    The point size is solved so the glyph's cap height is `cap_ratio` of the
    canvas. Placement is then corrected from the *rendered* result rather than
    from font metrics: `getbbox` reports the advance box, whose side bearings
    differ per glyph, so metric-based centring leaves a visible offset. Drawing
    once and measuring the ink box instead makes the margins balanced for any
    letter and any face.
    """
    font_path = find_font()
    target = size * cap_ratio

    probe = 100
    probe_font = ImageFont.truetype(font_path, probe)
    _l, probe_top, _r, probe_bottom = probe_font.getbbox(letter)
    point = max(1, round(probe * target / (probe_bottom - probe_top)))
    font = ImageFont.truetype(font_path, point)

    def draw_at(offset_x: float, offset_y: float) -> Image.Image:
        layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        ImageDraw.Draw(layer).text((offset_x, offset_y), letter, font=font,
                                   fill=(255, 255, 255, 255))
        return layer

    # Pass 1: draw at the origin and measure where the ink actually landed.
    first = draw_at(0.0, 0.0)
    ink = first.getbbox()
    if ink is None:
        return first
    ix0, iy0, ix1, iy1 = ink

    # Pass 2: shift so the ink box is centred (plus any optical adjustment).
    offset_x = (size - (ix1 - ix0)) / 2 - ix0 + size * LETTER_NUDGE_X
    offset_y = (size - (iy1 - iy0)) / 2 - iy0
    return draw_at(offset_x, offset_y)


def compose_icon(size: int, background: bool = True, ss: int = S, letter: str = "K") -> Image.Image:
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

    glyph = render_letter(big, letter)
    if not background:
        # Tint the glyph with the brand gradient for use on light backgrounds.
        tint = gradient(big, ACCENT_LIGHT, ACCENT_DEEP).convert("RGBA")
        tint.putalpha(glyph.getchannel("A"))
        glyph = tint
    base.alpha_composite(glyph)

    return base.resize((size, size), Image.LANCZOS)


def make_icon(size: int = CANVAS, background: bool = True) -> Image.Image:
    return compose_icon(size, background)


def save_png(img: Image.Image, path: str, size: int):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    img.resize((size, size), Image.LANCZOS).save(path, "PNG")
    print(f"  {path}  {size}x{size}")


def main():
    parser = argparse.ArgumentParser(description="Generate the Kilo App logo and icon set.")
    parser.add_argument("--letter", default="K", help="letter to draw on the tile (default: K)")
    parser.add_argument(
        "--cap",
        type=float,
        default=LETTER_CAP,
        help=f"letter cap height as a fraction of the tile (default: {LETTER_CAP})",
    )
    args = parser.parse_args()
    letter = args.letter

    os.makedirs(ICON_DIR, exist_ok=True)
    os.makedirs(ASSET_DIR, exist_ok=True)
    os.makedirs(PREVIEW_DIR, exist_ok=True)

    print(f"font: {find_font()}")
    print(f"rendering master icon with letter {letter!r}...")
    master = compose_icon(CANVAS, background=True, letter=letter)
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

    print("writing transparent letter glyph (for the website)...")
    glyph = compose_icon(512, background=False, letter=letter)
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
