#!/usr/bin/env bash
# Regenerates the committed PNG icons from the SVG sources (needs rsvg-convert, librsvg).
# Notifications and some launchers ignore SVG, hence PNGs. The badge is a monochrome alpha mask:
# Android draws only its shape in the status bar.
set -euo pipefail
cd "$(dirname "$0")/.."
rsvg-convert -w 192 -h 192 public/icon.svg -o public/icon-192.png
rsvg-convert -w 512 -h 512 public/icon.svg -o public/icon-512.png
rsvg-convert -w 96 -h 96 scripts/badge.svg -o public/badge-96.png
