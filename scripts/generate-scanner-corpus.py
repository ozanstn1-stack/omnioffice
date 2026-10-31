#!/usr/bin/env python3
"""Generates the photo-realistic scanner corpus (D4) and its ground truth.

Real camera photos cannot be committed to the repository (privacy, size), so
the corpus is rendered deterministically here: paper pages photographed on
desks with perspective, rotation, shadow gradients, vignetting, sensor noise,
busy backgrounds, soft focus and two-page spreads with a dark spine. The images
are small (360x480 single pages, 560x400 spreads) so the corpus stays light.

Usage:
    python3 scripts/generate-scanner-corpus.py

Requires only the Python standard library (no Pillow/numpy). The output is
`corpus/scanner/*.png` plus `corpus/scanner/ground-truth.json`.
"""
import json
import math
import os
import random
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "corpus", "scanner")


def clamp(value, low=0.0, high=255.0):
    return max(low, min(high, value))


def write_png(path, width, height, pixels):
    """Writes an 8-bit RGB PNG from a flat row-major bytearray."""
    stride = width * 3
    raw = bytearray()
    for y in range(height):
        raw.append(0)  # filter type 0
        raw += pixels[y * stride:(y + 1) * stride]

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    blob = b"\x89PNG\r\n\x1a\n"
    blob += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    blob += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    blob += chunk(b"IEND", b"")
    with open(path, "wb") as handle:
        handle.write(blob)


class Canvas:
    def __init__(self, width, height, background):
        self.width = width
        self.height = height
        self.pixels = bytearray(width * height * 3)
        for y in range(height):
            for x in range(width):
                self.set(x, y, background)

    def set(self, x, y, colour):
        if 0 <= x < self.width and 0 <= y < self.height:
            index = (y * self.width + x) * 3
            self.pixels[index] = int(clamp(colour[0]))
            self.pixels[index + 1] = int(clamp(colour[1]))
            self.pixels[index + 2] = int(clamp(colour[2]))

    def get(self, x, y):
        index = (y * self.width + x) * 3
        return (self.pixels[index], self.pixels[index + 1], self.pixels[index + 2])


def solve8(matrix, vector):
    size = 8
    for column in range(size):
        pivot = max(range(column, size), key=lambda row: abs(matrix[row][column]))
        if abs(matrix[pivot][column]) < 1e-9:
            return None
        matrix[column], matrix[pivot] = matrix[pivot], matrix[column]
        vector[column], vector[pivot] = vector[pivot], vector[column]
        for row in range(column + 1, size):
            factor = matrix[row][column] / matrix[column][column]
            for offset in range(column, size):
                matrix[row][offset] -= factor * matrix[column][offset]
            vector[row] -= factor * vector[column]
    result = [0.0] * size
    for row in range(size - 1, -1, -1):
        total = vector[row]
        for k in range(row + 1, size):
            total -= matrix[row][k] * result[k]
        result[row] = total / matrix[row][row]
    return result


def homography(destination, source):
    """Maps destination points (quad pixel corners) to source coordinates."""
    matrix = [[0.0] * 8 for _ in range(8)]
    vector = [0.0] * 8
    for index in range(4):
        x, y = destination[index]
        u, v = source[index]
        matrix[index * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u]
        vector[index * 2] = u
        matrix[index * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v]
        vector[index * 2 + 1] = v
    return solve8(matrix, vector)


def apply_h(matrix, x, y):
    denominator = matrix[6] * x + matrix[7] * y + 1.0
    if abs(denominator) < 1e-9:
        return None
    return (
        (matrix[0] * x + matrix[1] * y + matrix[2]) / denominator,
        (matrix[3] * x + matrix[4] * y + matrix[5]) / denominator,
    )


def inside(quad, point):
    sign = 0.0
    for index in range(4):
        ax, ay = quad[index]
        bx, by = quad[(index + 1) % 4]
        cross = (bx - ax) * (point[1] - ay) - (by - ay) * (point[0] - ax)
        current = 1.0 if cross > 0 else -1.0
        if abs(cross) < 1e-6:
            continue
        if sign == 0.0:
            sign = current
        elif current != sign:
            return False
    return True


def draw_page(canvas, quad, page_size=(100.0, 141.0), tone=(244, 241, 233), text=True,
              shade=None, curl=False, texture_noise=4):
    """Fills a page quad and maps page-local text and shading into it."""
    page_width, page_height = page_size
    inverse = homography(quad, [(0.0, 0.0), (page_width, 0.0), (page_width, page_height), (0.0, page_height)])
    if inverse is None:
        return
    xs = [p[0] for p in quad]
    ys = [p[1] for p in quad]
    seed = sum(ord(character) for character in str(quad)) & 0xFFFF
    for y in range(max(0, int(min(ys)) - 1), min(canvas.height, int(max(ys)) + 2)):
        for x in range(max(0, int(min(xs)) - 1), min(canvas.width, int(max(xs)) + 2)):
            if not inside(quad, (x, y)):
                continue
            mapped = apply_h(inverse, x, y)
            if mapped is None:
                continue
            page_x, page_y = mapped
            if not (0.0 <= page_x <= page_width and 0.0 <= page_y <= page_height):
                continue
            factor = 1.0
            if shade is not None:
                factor *= 1.0 - shade * (page_x / page_width)
            if curl:
                factor *= 1.0 - 0.18 * abs(page_x / page_width - 0.08)
            colour = [channel * factor for channel in tone]
            if text:
                margin = 10.0
                line_height = 4.2
                gap = 12.0
                line = 0.0
                while line < page_height - margin:
                    top = line + 4.0
                    if (8.0 <= page_x <= page_width - 8.0) and (top <= page_y <= top + 1.6) and line > 12.0:
                        width_scale = 0.55 + 0.35 * ((seed + line) % 5) / 5.0
                        if page_x <= 8.0 + (page_width - 16.0) * width_scale:
                            for channel in range(3):
                                colour[channel] *= 0.62
                    line += gap
            noise = ((seed * 1103515245 + x * 12345 + y * 6789) & 0x1F) - 16
            canvas.set(x, y, [channel + noise * 0.2 for channel in colour][:3])


def fill_rect_quad(canvas, quad, colour):
    xs = [p[0] for p in quad]
    ys = [p[1] for p in quad]
    for y in range(max(0, int(min(ys))), min(canvas.height, int(max(ys)) + 1)):
        for x in range(max(0, int(min(xs))), min(canvas.width, int(max(xs)) + 1)):
            if inside(quad, (x, y)):
                canvas.set(x, y, colour)


def add_noise(canvas, amount=6, seed=7):
    random.seed(seed)
    for index in range(0, len(canvas.pixels), 3):
        delta = random.randint(-amount, amount)
        for channel in range(3):
            canvas.pixels[index + channel] = int(clamp(canvas.pixels[index + channel] + delta))


def blur(canvas, radius=1):
    source = bytes(canvas.pixels)
    for y in range(canvas.height):
        for x in range(canvas.width):
            total = [0, 0, 0]
            count = 0
            for dy in range(-radius, radius + 1):
                for dx in range(-radius, radius + 1):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < canvas.width and 0 <= ny < canvas.height:
                        index = (ny * canvas.width + nx) * 3
                        for channel in range(3):
                            total[channel] += source[index + channel]
                        count += 1
            canvas.set(x, y, [value / count for value in total])


def vignette(canvas, strength=0.25):
    cx, cy = canvas.width / 2.0, canvas.height / 2.0
    maximum = math.hypot(cx, cy)
    for y in range(canvas.height):
        for x in range(canvas.width):
            distance = math.hypot(x - cx, y - cy) / maximum
            factor = 1.0 - strength * distance * distance
            r, g, b = canvas.get(x, y)
            canvas.set(x, y, (r * factor, g * factor, b * factor))


def textured_background(canvas, base, seed=3):
    random.seed(seed)
    for _ in range(40):
        x0 = random.randint(-20, canvas.width)
        y0 = random.randint(-20, canvas.height)
        width = random.randint(10, 90)
        height = random.randint(3, 24)
        tone = random.randint(-26, 22)
        for y in range(max(0, y0), min(canvas.height, y0 + height)):
            for x in range(max(0, x0), min(canvas.width, x0 + width)):
                r, g, b = canvas.get(x, y)
                canvas.set(x, y, (r + tone, g + tone, b + tone))


def rotated_rectangle(cx, cy, width, height, degrees):
    angle = math.radians(degrees)
    cos_a, sin_a = math.cos(angle), math.sin(angle)
    points = []
    for dx, dy in (
        (-width / 2.0, -height / 2.0),
        (width / 2.0, -height / 2.0),
        (width / 2.0, height / 2.0),
        (-width / 2.0, height / 2.0),
    ):
        points.append((
            round(cx + dx * cos_a - dy * sin_a, 2),
            round(cy + dx * sin_a + dy * cos_a, 2),
        ))
    return points


def spread_pages(outer, gutter_ratio=0.012):
    tl, tr, br, bl = outer
    middle_top = ((tl[0] + tr[0]) / 2.0, (tl[1] + tr[1]) / 2.0)
    middle_bottom = ((bl[0] + br[0]) / 2.0, (bl[1] + br[1]) / 2.0)
    width = math.hypot(tr[0] - tl[0], tr[1] - tl[1])
    gutter = width * gutter_ratio
    dx = (tr[0] - tl[0]) / width
    dy = (tr[1] - tl[1]) / width
    left = [
        tl,
        (middle_top[0] - dx * gutter, middle_top[1] - dy * gutter),
        (middle_bottom[0] - dx * gutter, middle_bottom[1] - dy * gutter),
        bl,
    ]
    right = [
        (middle_top[0] + dx * gutter, middle_top[1] + dy * gutter),
        tr,
        br,
        (middle_bottom[0] + dx * gutter, middle_bottom[1] + dy * gutter),
    ]
    return left, right


def write_case(images, name, canvas, expected_pages, conditions, text=True):
    path = os.path.join(OUT, name)
    write_png(path, canvas.width, canvas.height, canvas.pixels)
    images.append({
        "file": name,
        "conditions": conditions,
        "pages": [[[round(x, 1), round(y, 1)] for x, y in page] for page in expected_pages],
    })
    print(f"{name}: {canvas.width}x{canvas.height}, {len(expected_pages)} page(s)")


def single(background, quad, tone=(246, 243, 236), shade=None, curl=False, text=True,
           noise=6, blur_radius=0, vignette_strength=0.0, texture=False, seed=3):
    canvas = Canvas(360, 480, background)
    if texture:
        textured_background(canvas, background, seed=seed)
    draw_page(canvas, quad, tone=tone, shade=shade, curl=curl, text=text)
    add_noise(canvas, amount=noise, seed=seed)
    if blur_radius:
        blur(canvas, radius=blur_radius)
    if vignette_strength:
        vignette(canvas, strength=vignette_strength)
    return canvas


def spread(background, outer, tone_left=(246, 243, 236), tone_right=(243, 240, 232),
           shade_left=None, shade_right=None, texture=False, seed=9):
    canvas = Canvas(560, 400, background)
    if texture:
        textured_background(canvas, background, seed=seed)
    left, right = spread_pages(outer)
    draw_page(canvas, left, tone=tone_left, shade=shade_left, text=True)
    draw_page(canvas, right, tone=tone_right, shade=shade_right, text=True)
    add_noise(canvas, amount=6, seed=seed)
    vignette(canvas, strength=0.2)
    return canvas, [left, right]


def main():
    os.makedirs(OUT, exist_ok=True)
    images = []

    write_case(images, "01_desk_dark_perspective.png",
               single((58, 62, 68), ((70, 60), (300, 48), (315, 400), (55, 420)), shade=0.12),
               [((70, 60), (300, 48), (315, 400), (55, 420))], ["dark desk", "perspective"])
    write_case(images, "02_desk_light_shadow.png",
               single((204, 198, 188), ((60, 55), (300, 70), (290, 415), (55, 400)), shade=0.3, texture=True),
               [((60, 55), (300, 70), (290, 415), (55, 400))], ["light desk", "shadow"])
    write_case(images, "03_rotated_12deg.png",
               single((60, 64, 70), rotated_rectangle(180, 240, 232, 322, 12)),
               [rotated_rectangle(180, 240, 232, 322, 12)], ["rotation 12", "dark desk"])
    write_case(images, "04_low_contrast.png",
               single((178, 176, 170), ((80, 70), (285, 66), (290, 410), (75, 405)), tone=(214, 210, 202), shade=0.08),
               [((80, 70), (285, 66), (290, 410), (75, 405))], ["low contrast", "soft edge"])
    busy = Canvas(360, 480, (92, 90, 86))
    textured_background(busy, (92, 90, 86), seed=11)
    draw_page(busy, ((85, 65), (295, 78), (300, 405), (70, 390)))
    add_noise(busy, amount=8, seed=11)
    write_case(images, "05_busy_desk.png", busy,
               [((85, 65), (295, 78), (300, 405), (70, 390))], ["busy desk", "perspective"])
    write_case(images, "06_warm_light.png",
               single((118, 96, 74), ((72, 58), (292, 62), (298, 408), (62, 402)), tone=(242, 234, 214),
                      shade=0.18, vignette_strength=0.35),
               [((72, 58), (292, 62), (298, 408), (62, 402))], ["warm light", "vignette"])
    write_case(images, "07_cool_color.png",
               single((62, 76, 96), ((76, 66), (296, 58), (290, 406), (66, 414)), tone=(236, 240, 246)),
               [((76, 66), (296, 58), (290, 406), (66, 414))], ["cool light", "color cast"])
    write_case(images, "08_strong_perspective.png",
               single((70, 72, 76), ((112, 62), (288, 112), (258, 430), (46, 382)), shade=0.2),
               [((112, 62), (288, 112), (258, 430), (46, 382))], ["strong perspective"])
    write_case(images, "09_paper_texture.png",
               single((72, 74, 78), ((66, 54), (294, 66), (306, 414), (58, 396)), shade=0.15, noise=9),
               [((66, 54), (294, 66), (306, 414), (58, 396))], ["paper texture", "text lines"])
    write_case(images, "10_soft_focus.png",
               single((64, 66, 70), ((72, 62), (296, 54), (308, 404), (58, 416)), shade=0.1, blur_radius=1),
               [((72, 62), (296, 54), (308, 404), (58, 416))], ["defocus"])
    write_case(images, "11_edge_crop.png",
               single((66, 68, 72), ((96, 70), (358, 82), (352, 402), (88, 414))),
               [((96, 70), (358, 82), (352, 402), (88, 414))], ["page near the edge"])

    outer = ((62, 72), (502, 58), (510, 342), (52, 356))
    canvas, pages = spread((64, 66, 70), outer, shade_left=0.16)
    write_case(images, "12_spread_dark.png", canvas, pages, ["two-page spread", "dark desk", "spine shadow"])
    canvas, pages = spread(((198, 192, 182)), outer, shade_right=0.22, texture=True, seed=13)
    write_case(images, "13_spread_light_shadow.png", canvas, pages, ["two-page spread", "light desk", "shadow"])
    rotated_outer = rotated_rectangle(280, 200, 448, 288, 7)
    canvas, pages = spread((60, 62, 68), rotated_outer, shade_left=0.1)
    write_case(images, "14_spread_rotated.png", canvas, pages, ["two-page spread", "rotation 7"])

    with open(os.path.join(OUT, "ground-truth.json"), "w", encoding="utf-8") as handle:
        json.dump({"version": 1, "images": images}, handle, indent=2)
        handle.write("\n")
    print(f"wrote {len(images)} images and ground-truth.json to {OUT}")


if __name__ == "__main__":
    main()
