#!/usr/bin/env python3

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont


REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_1X = REPO_ROOT / "src-tauri" / "dmg-background.png"
OUT_2X = REPO_ROOT / "src-tauri" / "dmg-background@2x.png"

SIZE_1X = (660, 400)
SCALE = 2
SIZE_2X = (SIZE_1X[0] * SCALE, SIZE_1X[1] * SCALE)

FONT_PATH = "/System/Library/Fonts/HelveticaNeue.ttc"
FONT_REGULAR = 0
FONT_BOLD = 1
FONT_LIGHT = 7


def lerp(a: int, b: int, t: float) -> int:
    return round(a + (b - a) * t)


def vertical_gradient(size: tuple[int, int], top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    width, height = size
    image = Image.new("RGBA", size)
    pixels = image.load()
    for y in range(height):
        t = y / max(height - 1, 1)
        row = tuple(lerp(top[i], bottom[i], t) for i in range(3)) + (255,)
        for x in range(width):
            pixels[x, y] = row
    return image


def scaled_box(box: tuple[float, float, float, float], scale: int) -> tuple[int, int, int, int]:
    return tuple(round(v * scale) for v in box)


def rounded_panel(
    image: Image.Image,
    box: tuple[float, float, float, float],
    radius: float,
    fill: tuple[int, int, int, int],
    blur: float,
    scale: int,
) -> None:
    overlay = Image.new("RGBA", image.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    draw.rounded_rectangle(scaled_box(box, scale), radius=round(radius * scale), fill=fill)
    overlay = overlay.filter(ImageFilter.GaussianBlur(radius=blur * scale))
    image.alpha_composite(overlay)


def ellipse_glow(
    image: Image.Image,
    box: tuple[float, float, float, float],
    fill: tuple[int, int, int, int],
    blur: float,
    scale: int,
) -> None:
    overlay = Image.new("RGBA", image.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    draw.ellipse(scaled_box(box, scale), fill=fill)
    overlay = overlay.filter(ImageFilter.GaussianBlur(radius=blur * scale))
    image.alpha_composite(overlay)


def centered_text(
    draw: ImageDraw.ImageDraw,
    center_x: float,
    top_y: float,
    text: str,
    font: ImageFont.FreeTypeFont,
    fill: tuple[int, int, int, int],
) -> None:
    left, top, right, bottom = draw.textbbox((0, 0), text, font=font)
    width = right - left
    draw.text((center_x - width / 2, top_y), text, font=font, fill=fill)


def draw_arrow(draw: ImageDraw.ImageDraw, scale: int) -> None:
    y = round(168 * scale)
    start_x = round(232 * scale)
    end_x = round(438 * scale)
    shaft_width = round(2.5 * scale)
    head = round(10 * scale)
    draw.line((start_x, y, end_x, y), fill=(244, 247, 252, 232), width=shaft_width)
    draw.polygon(
        [
            (end_x, y),
            (end_x - head, y - head // 2),
            (end_x - head, y + head // 2),
        ],
        fill=(244, 247, 252, 232),
    )


def build_background(scale: int) -> Image.Image:
    size = (SIZE_1X[0] * scale, SIZE_1X[1] * scale)
    image = vertical_gradient(size, (27, 31, 44), (21, 25, 37))

    rounded_panel(
        image,
        (68, 102, 592, 260),
        radius=42,
        fill=(255, 255, 255, 20),
        blur=22,
        scale=scale,
    )
    ellipse_glow(
        image,
        (98, 118, 264, 224),
        fill=(255, 255, 255, 62),
        blur=24,
        scale=scale,
    )
    ellipse_glow(
        image,
        (396, 118, 562, 224),
        fill=(255, 255, 255, 62),
        blur=24,
        scale=scale,
    )
    rounded_panel(
        image,
        (104, 214, 256, 252),
        radius=18,
        fill=(245, 247, 252, 185),
        blur=10,
        scale=scale,
    )
    rounded_panel(
        image,
        (404, 214, 556, 252),
        radius=18,
        fill=(245, 247, 252, 185),
        blur=10,
        scale=scale,
    )
    ellipse_glow(
        image,
        (178, 258, 482, 360),
        fill=(126, 149, 193, 38),
        blur=30,
        scale=scale,
    )

    draw = ImageDraw.Draw(image)
    draw_arrow(draw, scale)

    title_font = ImageFont.truetype(FONT_PATH, 24 * scale, index=FONT_BOLD)
    subtitle_font = ImageFont.truetype(FONT_PATH, 14 * scale, index=FONT_LIGHT)

    centered_text(
        draw,
        center_x=330 * scale,
        top_y=268 * scale,
        text="Drag ONESHIM to Applications",
        font=title_font,
        fill=(246, 248, 252, 248),
    )
    centered_text(
        draw,
        center_x=330 * scale,
        top_y=306 * scale,
        text="Drop the app icon onto the Applications folder",
        font=subtitle_font,
        fill=(196, 205, 222, 236),
    )

    return image


def main() -> None:
    image_2x = build_background(SCALE)
    image_1x = image_2x.resize(SIZE_1X, Image.Resampling.LANCZOS).filter(
        ImageFilter.UnsharpMask(radius=1.0, percent=140, threshold=2)
    )

    OUT_2X.parent.mkdir(parents=True, exist_ok=True)
    image_2x.save(OUT_2X)
    image_1x.save(OUT_1X)

    print(f"wrote {OUT_1X}")
    print(f"wrote {OUT_2X}")


if __name__ == "__main__":
    main()
