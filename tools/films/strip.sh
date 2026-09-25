#!/bin/sh
# strip.sh DIR NAME GEOMETRY [OUT] [SCALE] [FIRST] [LAST] -- lays the frames
# DIR/NAME-000.png, NAME-001.png, ... (FIRST to LAST, default all) side by
# side, each cropped to GEOMETRY (WxH+X+Y) and scaled to SCALE (default
# 50%), into OUT (default DIR/strip-NAME.png).
set -eu
DIR=$1; NAME=$2; GEO=$3; OUT=${4:-$DIR/strip-$NAME.png}; SCALE=${5:-50%}
FIRST=${6:-0}; LAST=${7:-999}
set --
i=$FIRST
while [ "$i" -le "$LAST" ]; do
  f=$(printf '%s/%s-%03d.png' "$DIR" "$NAME" "$i")
  [ -f "$f" ] || break
  set -- "$@" "$f"
  i=$((i + 1))
done
[ $# -gt 0 ] || { echo "no frames $DIR/$NAME-NNN.png" >&2; exit 1; }
magick "$@" -crop "$GEO" +repage -resize "$SCALE" +append "$OUT"
echo "$OUT"
