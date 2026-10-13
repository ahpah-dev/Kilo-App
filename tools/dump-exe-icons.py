"""Render the app icon PNGs embedded in a PE's .rsrc section.

Pillow writes .ico frames as PNG streams, so each RT_ICON payload begins with
the PNG signature and ends with IEND. This extracts those streams directly.
"""
import io
import struct
import sys

from PIL import Image

PNG_SIG = b"\x89PNG\r\n\x1a\n"
IEND = b"IEND\xaeB`\x82"


def rsrc_range(d):
    e = struct.unpack_from("<I", d, 0x3C)[0]
    coff = e + 4
    nsec, = struct.unpack_from("<H", d, coff + 2)
    optsz, = struct.unpack_from("<H", d, coff + 16)
    opt = coff + 20
    magic, = struct.unpack_from("<H", d, opt)
    dd = opt + (112 if magic == 0x20B else 96)
    sec = opt + optsz
    for i in range(nsec):
        o = sec + i * 40
        nm = d[o:o + 8].rstrip(b"\0").decode("ascii", "replace")
        vsz, va, rsz, rp = struct.unpack_from("<IIII", d, o + 8)
        if nm == ".rsrc":
            return rp, rp + rsz
    return None, None


def main(path, out):
    d = open(path, "rb").read()
    start, end = rsrc_range(d)
    if start is None:
        print("no .rsrc")
        return 1

    tiles = []
    pos = start
    while True:
        i = d.find(PNG_SIG, pos, end)
        if i < 0:
            break
        j = d.find(IEND, i, end)
        if j < 0:
            break
        blob = d[i:j + len(IEND)]
        pos = j + len(IEND)
        try:
            im = Image.open(io.BytesIO(blob))
            im.load()
            c = im.convert("RGBA")
            w, h = c.size
            if w > 300 or h > 300:
                continue
            print(f"  PNG {w}x{h} @ {hex(i)} ({len(blob)}B)  edge={c.getpixel((1, h // 2))}  centre={c.getpixel((w // 2, h // 2))}")
            tiles.append(c)
        except Exception:
            continue

    print(f"{path}: {len(tiles)} embedded PNG icon frame(s)")
    if tiles:
        tiles.sort(key=lambda t: t.size[0])
        S = 96
        sheet = Image.new("RGBA", (S * len(tiles) + 16 * (len(tiles) + 1), S + 32), (245, 246, 250, 255))
        x = 16
        for t in tiles:
            sheet.alpha_composite(t.resize((S, S), Image.LANCZOS), (x, 16))
            x += S + 16
        sheet.save(out, "PNG")
        print(f"  sheet -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
