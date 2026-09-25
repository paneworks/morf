#!/bin/sh
# strip.sh DIR NAME GEOMETRY [OUT] [SCALE] -- lays the frames DIR/NAME-000.png,
# NAME-001.png, ... side by side, each cropped to GEOMETRY (WxH+X+Y) and
# scaled to SCALE (default 50%), into OUT (default DIR/strip-NAME.png).
set -eu
DIR=$1; NAME=$2; GEO=$3; OUT=${4:-$DIR/strip-$NAME.png}; SCALE=${5:-50%}
set --
for f in "$DIR/$NAME"-[0-9][0-9][0-9].png; do set -- "$@" "$f"; done
[ $# -gt 0 ] || { echo "no frames $DIR/$NAME-NNN.png" >&2; exit 1; }
magick "$@" -crop "$GEO" +repage -resize "$SCALE" +append "$OUT"
echo "$OUT"
