"""Render TestBackend color captures for offline visual inspection (Pillow)."""
import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


def render(path, destination):
    capture = json.loads(path.read_text(encoding="utf-8"))
    width, height = capture["width"], capture["height"]
    cw, ch = 11, 24
    regular = ImageFont.truetype("C:/Windows/Fonts/consola.ttf", 18)
    bold = ImageFont.truetype("C:/Windows/Fonts/consolab.ttf", 18)
    fallback = ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 18)
    image = Image.new("RGB", (width * cw, height * ch), "#11151b")
    draw = ImageDraw.Draw(image)
    cells = capture["cells"]
    for index, cell in enumerate(cells):
        x, y = index % width * cw, index // width * ch
        bg = cell["bg"] if cell["bg"].startswith("#") else "#11151b"
        draw.rectangle((x, y, x + cw - 1, y + ch - 1), fill=bg)
    for index, cell in enumerate(cells):
        symbol = cell["s"]
        if not symbol.strip():
            continue
        x, y = index % width * cw, index // width * ch
        fg = cell["fg"] if cell["fg"].startswith("#") else "#dde6f0"
        font = fallback if any(ord(c) > 0x2fff for c in symbol) else bold if cell["bold"] else regular
        draw.text((x, y + 2), symbol, font=font, fill=fg)
    output = destination / (path.stem + ".png")
    image.save(output)
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--names", nargs="*")
    args = parser.parse_args()
    destination = args.output or args.source / "png"
    destination.mkdir(parents=True, exist_ok=True)
    paths = [args.source / (name + ".json") for name in args.names] if args.names else sorted(args.source.glob("*.json"))
    images = []
    for path in paths:
        picture = render(path, destination)
        picture.thumbnail((720, 420))
        images.append((path.stem, picture))
    for page in range(0, len(images), 12):
        group = images[page:page + 12]
        sheet = Image.new("RGB", (1440, ((len(group) + 1) // 2) * 450), "#11151b")
        draw = ImageDraw.Draw(sheet)
        for index, (name, picture) in enumerate(group):
            x, y = index % 2 * 720, index // 2 * 450
            draw.text((x + 8, y + 4), name, fill="white")
            sheet.paste(picture, (x, y + 25))
        sheet.save(destination / f"contact-{page // 12 + 1}.png")
    print(f"Rendered {len(images)} captures into {destination}")


if __name__ == "__main__":
    main()
