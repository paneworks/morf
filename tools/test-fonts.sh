#!/usr/bin/env bash
set -euo pipefail

root=${1:?font fixture directory required}
mkdir -p "$root/fonts" "$root/cache"
root=$(realpath -e "$root")
font="$root/fonts/MaterialSymbolsRounded.ttf"
checksum=95b24392bb49efd1bc3e92cff4e2452ad094461bab7c97e7d8723fab97e330ca
valid_font() {
  printf '%s  %s\n' "$checksum" "$font" | sha256sum --check --status
}
if ! valid_font 2>/dev/null; then
  curl --fail --location --silent --show-error --retry 3 --globoff \
    'https://raw.githubusercontent.com/google/material-design-icons/737e3324305806514d7909874fa1818ae1808232/variablefont/MaterialSymbolsRounded%5BFILL,GRAD,opsz,wght%5D.ttf' \
    --output "$font"
  valid_font
fi
cat > "$root/fonts.conf" <<EOF
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
  <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
  <dir>$root/fonts</dir>
  <cachedir>$root/cache</cachedir>
</fontconfig>
EOF
test "$(FONTCONFIG_FILE="$root/fonts.conf" fc-match --format '%{family}' 'Material Symbols Rounded')" = 'Material Symbols Rounded'
printf '%s\n' "$root/fonts.conf"
