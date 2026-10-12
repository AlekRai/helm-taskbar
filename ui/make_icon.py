"""Draws the Helm icon (a ship's wheel on a dark tile) into icon.png and icon.ico."""
import math
from pathlib import Path

from PIL import Image, ImageDraw

S = 1024
BG = (24, 24, 28, 255)
ACCENT = (76, 160, 255, 255)

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
d.rounded_rectangle((0, 0, S - 1, S - 1), radius=230, fill=BG)

c = S / 2
R_OUT, R_IN, R_HUB = 250, 200, 85
d.ellipse((c - R_OUT, c - R_OUT, c + R_OUT, c + R_OUT), fill=ACCENT)
d.ellipse((c - R_IN, c - R_IN, c + R_IN, c + R_IN), fill=BG)

for k in range(8):
    a = math.radians(k * 45)
    ex, ey = c + math.cos(a) * 385, c + math.sin(a) * 385
    d.line((c, c, ex, ey), fill=ACCENT, width=34)
    d.ellipse((ex - 50, ey - 50, ex + 50, ey + 50), fill=ACCENT)

# Clear a ring between the hub and the rim so the spokes read as part of the wheel.
d.ellipse((c - R_HUB, c - R_HUB, c + R_HUB, c + R_HUB), fill=ACCENT)
d.ellipse((c - 40, c - 40, c + 40, c + 40), fill=BG)

here = Path(__file__).parent
img.resize((256, 256), Image.LANCZOS).save(here / "icon.png")
img.save(here / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
