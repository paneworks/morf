#!/bin/sh
# Root installs the verifier and PAM rules, then enrolls interactively.
set -eu
[ "$(id -u)" = 0 ] || { echo 'Run with sudo; NixOS uses programs.morf.pattern instead.' >&2; exit 1; }
[ ! -e /etc/NIXOS ] || { echo 'On NixOS enable programs.morf.pattern, rebuild, then run sudo morf-pattern setup USER.' >&2; exit 1; }
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
pattern_build=$(mktemp -d)
trap 'rm -rf -- "$pattern_build"' EXIT
cc -std=c11 -O2 -Wall -Wextra -Werror -fstack-protector-strong -D_FORTIFY_SOURCE=2 \
  "$here/check.c" -o "$pattern_build/morf-pattern-check" -lcrypt
python3 "$here/install.py" "$pattern_build/morf-pattern-check"
/usr/local/bin/morf-pattern setup "${1:-${SUDO_USER:?Pass the account name}}"
