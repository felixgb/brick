#!/bin/sh
# Renders brick.svg into png/ for the Linux app (window icon, and the hicolor
# theme on desktops without SVG icons). Needs rsvg-convert (librsvg).
# Only needed when the icon changes; the PNGs are checked in.
set -eu
cd "$(dirname "$0")"
mkdir -p png
for size in 16 32 48 64 128 256; do
    rsvg-convert -w $size -h $size brick.svg -o png/brick-$size.png
done
echo "Wrote $(pwd)/png"
