#!/usr/bin/env python3
"""Fail a native Slint UI build if required 22-page theme/viewport renders are absent or blank.

This is image-render QA, NOT mouse-click testing, layout OCR or a broker acceptance test.
Only Python standard library is required (including on Windows build agents).
"""
from pathlib import Path
import hashlib

ROOT = Path(__file__).resolve().parents[1]
IMAGES = ROOT / "test-output" / "native-ui"
SIZES = (("", 1120, 820), ("-small", 880, 700))
PALETTES = ("dark", "white", "smoke")
SELECTED = {0, 6, 14, 17, 18, 19, 21}
RESPONSIVE = ((1024,768),(1280,800),(1366,768),(1600,900),(1920,1080))


def image_path(page: int, size: str, palette: str) -> Path:
    suffix = "" if palette == "dark" else f"-{palette}"
    return IMAGES / f"dashboard-page-{page}{size}{suffix}.ppm"


def check_ppm(path: Path, width: int, height: int) -> str:
    if not path.is_file():
        raise AssertionError(f"Missing native UI render: {path}")
    with path.open("rb") as file:
        if file.readline() != b"P6\n":
            raise AssertionError(f"Not a P6 image: {path}")
        expected_dimensions = f"{width} {height}\n".encode()
        if file.readline() != expected_dimensions:
            raise AssertionError(f"Unexpected image dimensions: {path}")
        if file.readline() != b"255\n":
            raise AssertionError(f"Not RGB 8-bit: {path}")
        pixels = file.read()
    if len(pixels) != width * height * 3:
        raise AssertionError(f"Truncated/corrupt RGB render: {path}")
    # A frozen/blank window can still have valid dimensions. Sample actual RGB
    # triplets across the frame, not just a potentially solid background corner.
    step = 983 * 3
    rgb_samples = {pixels[i:i + 3] for i in range(0, len(pixels) - 2, step)}
    if len(rgb_samples) < 8:
        raise AssertionError(f"Render has too few color variations (possibly blank): {path}")
    return hashlib.sha256(pixels).hexdigest()


def assert_widescreen_work_area_occupied(path: Path, width: int, height: int) -> None:
    """Catch the huge right-side blank panel visible in real UI-09 1920px shots.

    A correctly sized chart or native trading console has diverse pixel colors
    in the central-right viewport, unlike the single-color blank background.
    This is a geometric smoke check, not OCR or a proof of pixel-perfect match.
    """
    with path.open("rb") as file:
        assert file.readline() == b"P6\n"
        assert file.readline() == f"{width} {height}\n".encode()
        assert file.readline() == b"255\n"
        pixels = file.read()
    colors = set()
    for y in range(int(height * .33), int(height * .66), 7):
        for x in range(int(width * .67), int(width * .92), 7):
            i = (y * width + x) * 3
            colors.add(pixels[i:i + 3])
    if len(colors) < 16:
        raise AssertionError(
            f"Widescreen dashboard still leaves right work area blank: "
            f"{path}, {len(colors)} colors in 67-92% viewport"
        )


def assert_laptop_content_left_anchored(path: Path, width: int, height: int) -> None:
    """Detect laptop mode where ScrollView centers its content off screen."""
    with path.open("rb") as file:
        assert file.readline() == b"P6\n"
        assert file.readline() == f"{width} {height}\n".encode()
        assert file.readline() == b"255\n"
        pixels=file.read()
    # Laptop 1024px viewport begins at x≈190 after the left rail. The
    # command banner must appear at x200-280 rather than starting at x≈320.
    colors=set()
    for y in range(104,137,5):
        for x in range(200,281,5):
            at=(y*width+x)*3
            colors.add(pixels[at:at+3])
    if len(colors)<5:
        raise AssertionError(
            f"Laptop dashboard is horizontally centered/cropped: "
            f"{path} has only {len(colors)} colors at the expected left header"
        )


def main() -> None:
    validated = 0
    for size, width, height in SIZES:
        for page in range(22):
            dark = check_ppm(image_path(page, size, "dark"), width, height)
            validated += 1
            if page not in SELECTED:
                continue
            white = check_ppm(image_path(page, size, "white"), width, height)
            smoke = check_ppm(image_path(page, size, "smoke"), width, height)
            validated += 2
            if len({dark, white, smoke}) != 3:
                raise AssertionError(f"UI palette did not change for page={page}, size={size}")
    for palette in PALETTES:
        check_ppm(ROOT / "test-output" / "reference-preview" /
                  f"reference-command-center-{palette}.ppm", 1540, 920)
    for width,height in RESPONSIVE:
        hashes=[]
        for palette in PALETTES:
            path=ROOT/"test-output"/"responsive-preview"/f"dashboard-{width}x{height}-{palette}.ppm"
            hashes.append(check_ppm(path,width,height))
        if len(set(hashes)) != 3:
            raise AssertionError(f"Broken palette at viewport {width}x{height}")
        if width == 1024:
            assert_laptop_content_left_anchored(
                ROOT/"test-output"/"responsive-preview"/f"dashboard-{width}x{height}-dark.ppm",
                width,height
            )
        if width >= 1366:
            assert_widescreen_work_area_occupied(
                ROOT/"test-output"/"responsive-preview"/f"dashboard-{width}x{height}-dark.ppm",
                width,height
            )
    if validated != 72:
        raise AssertionError(f"Expected 72 saved renders, checked {validated}")
    print(f"Native Slint render QA OK: {validated} RGB screenshots; "
          "22 pages × 2 sizes × 3 rendered palettes (132 runtime combinations).")
    print("This check does not certify label clipping, real mouse events or MT5 Demo.")


if __name__ == "__main__":
    main()
