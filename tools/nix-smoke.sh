#!/usr/bin/env bash
set -euo pipefail

engine=$(realpath -e "${1:?engine output required}")
library=$(realpath -e "${2:?library output required}")
source_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$source_dir/Cargo.toml")
scratch=$(mktemp -d "${TMPDIR:-/tmp}/morf-nix-smoke.XXXXXXXX")
trap 'rm -rf "$scratch"' EXIT
export HOME="$scratch/home"
export XDG_CONFIG_HOME="$HOME/config"
export XDG_DATA_HOME="$HOME/data"
export XDG_STATE_HOME="$HOME/state"
export XDG_CACHE_HOME="$HOME/cache"
export XDG_DATA_DIRS="$scratch/no-system-data"
unset MORF_RUNTIME_PATH LUA_PATH LUA_CPATH
mkdir -p "$HOME"
cp "$source_dir/library/tests/kit_spec.lua" "$scratch/kit_spec.lua"
cp "$source_dir/library/tests/frecency_spec.lua" "$scratch/frecency_spec.lua"
cd "$scratch"

test "$("$engine/bin/morf" --version)" = "morf $version"
test -x "$engine/bin/morf-keyring"
"$engine/bin/morf" --help
test -f "$library/share/morf/library/lib/kit/contract.lua"
test -f "$library/share/morf/library/types/morf.lua"
test -f "$library/share/morf/library/types/morf/ui.lua"
test -f "$library/share/morf/library/luarc.template.json"
test "$(realpath "$engine/share/morf/library")" = "$library/share/morf/library"
"$engine/bin/morf" types "$scratch/types"
diff -r "$library/share/morf/library/types" "$scratch/types"
"$engine/bin/morf" test --no-dbus "$scratch/kit_spec.lua" "$scratch/frecency_spec.lua"
printf 'Installed engine and Lua library smoke checks passed\n'
