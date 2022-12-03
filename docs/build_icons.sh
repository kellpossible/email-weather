#!/bin/sh

cd static

# Create favicon
inkscape -w 16 -h 16 -o logo16.png logo-inkscape.svg
inkscape -w 32 -h 32 -o logo32.png logo-inkscape.svg
inkscape -w 48 -h 48 -o logo48.png logo-inkscape.svg
convert logo16.png logo32.png logo48.png favicon.ico
rm logo*.png

# Create plain svg
inkscape -l -o logo.svg logo-inkscape.svg
