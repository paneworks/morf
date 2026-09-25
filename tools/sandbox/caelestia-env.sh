#!/bin/sh
# caelestia-env.sh PKG -- the environment caelestia's flake launcher would set,
# as shell `export` lines, and QS=<the Quickshell to start>.
#
# PKG is `nix build <caelestia clone>#caelestia-shell`. Its bin/caelestia-shell
# is two nix binary wrappers around Quickshell's own wrapper (qs): the outer
# adds the plugin's QML import path and Qt plugins, the inner the fonts, the
# helper library dir, the xkb rules -- and puts the real ddcutil,
# brightnessctl, nmcli, ... first on PATH. That PATH is exactly what the
# sandbox must not have, so the launcher is never run: its settings are read
# back from the wrappers (they embed the makeCWrapper call that made them),
# PATH left out, and qs is started directly.
set -eu
wrap() {
  strings "$1" | sed -n "/^makeCWrapper '/,/^# (Use/p" | tr -d "'\\\\" |
  while read -r a b c d; do
    case "$a" in
      makeCWrapper) echo "TARGET=$b" ;;
      --prefix) [ "$b" = PATH ] || echo "export $b=\"$d\${$b:+:\$$b}\"" ;;
      --set) echo "export $b=\"$c\"" ;;
    esac
  done
}
outer=$(wrap "$1/bin/caelestia-shell")
inner_bin=$(echo "$outer" | sed -n 's/^TARGET=//p')
inner=$(wrap "$inner_bin")
echo "$outer" | grep -v '^TARGET='
echo "$inner" | grep -v '^TARGET='
echo "QS=$(echo "$inner" | sed -n 's/^TARGET=//p')"
