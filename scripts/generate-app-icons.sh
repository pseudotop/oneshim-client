#!/usr/bin/env bash

set -euo pipefail

SOURCE_SVG="${1:-assets/brand/logo-icon.svg}"
OUTPUT_DIR="${2:-crates/oneshim-app/assets}"

if [[ ! -f "$SOURCE_SVG" ]]; then
  echo "[ERROR] Source logo not found: $SOURCE_SVG" >&2
  exit 1
fi

if ! command -v magick >/dev/null 2>&1; then
  echo "[ERROR] ImageMagick (magick) is required." >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "[ERROR] python3 is required." >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

magick "$SOURCE_SVG" -background none -resize 1024x1024 PNG32:"$OUTPUT_DIR/icon.png"
magick "$OUTPUT_DIR/icon.png" -define icon:auto-resize=256,128,64,48,32,16 "$OUTPUT_DIR/icon.ico"

python3 - "$OUTPUT_DIR/icon.png" "$OUTPUT_DIR/icon.icns" <<'PY'
import sys
from PIL import Image

png_path = sys.argv[1]
icns_path = sys.argv[2]

img = Image.open(png_path).convert("RGBA")
img.save(
    icns_path,
    format="ICNS",
    sizes=[(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)],
)
PY

echo "[OK] Generated:"
echo "  - $OUTPUT_DIR/icon.png"
echo "  - $OUTPUT_DIR/icon.ico"
echo "  - $OUTPUT_DIR/icon.icns"
