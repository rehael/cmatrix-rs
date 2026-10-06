"""Stack a horizontal band of numbered wt-run.ps1 frames into one labelled contact sheet.

usage: sheet.py <frames_dir> <out.png> <first> <last> [step] [top_frac] [bottom_frac] [zoom]

The -F box is centred vertically, so a band of 0.44 to 0.62 of the window height
covers it. Needs Pillow (PIL).
"""
import glob
import os
import sys

from PIL import Image, ImageDraw

d, out, first, last = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
step = int(sys.argv[5]) if len(sys.argv) > 5 else 1
top, bottom = (float(sys.argv[6]), float(sys.argv[7])) if len(sys.argv) > 7 else (0.40, 0.68)
zoom = int(sys.argv[8]) if len(sys.argv) > 8 else 1

files = sorted(glob.glob(os.path.join(d, "[0-9]*.png")))[first : last + 1 : step]
bands = []
for f in files:
    im = Image.open(f).convert("RGB")
    w, h = im.size
    band = im.crop((0, int(h * top), w, int(h * bottom)))
    band = band.resize((band.width * zoom, band.height * zoom), Image.NEAREST)
    canvas = Image.new("RGB", (band.width, band.height + 16), (60, 0, 60))
    canvas.paste(band, (0, 16))
    ImageDraw.Draw(canvas).text((4, 2), os.path.basename(f), fill=(255, 255, 0))
    bands.append(canvas)
sheet = Image.new("RGB", (max(b.width for b in bands), sum(b.height for b in bands)))
y = 0
for b in bands:
    sheet.paste(b, (0, y))
    y += b.height
sheet.save(out)
print(out, sheet.size, len(bands), "frames")
