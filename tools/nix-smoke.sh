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

# The bundled docs: stamped with this version, headed by `morf --version`, and
# the package's own bundle printed over a stale one in the data folders.
reported=$("$engine/bin/morf" --version)
test "$(cat "$library/share/morf/library/docs/VERSION")" = "$version"
mkdir -p "$XDG_DATA_HOME/morf/library/docs"
printf 'STALE GUIDE\n' > "$XDG_DATA_HOME/morf/library/docs/UI.md"
printf '0.0.1-stale\n' > "$XDG_DATA_HOME/morf/library/docs/VERSION"
test "$("$engine/bin/morf" docs | head -n 1)" = "# $reported API reference"
"$engine/bin/morf" docs --ui > "$scratch/ui.md"
test "$(head -n 1 "$scratch/ui.md")" = "# $reported UI guide"
tail -n +4 "$scratch/ui.md" | cmp - "$source_dir/docs/UI.md"
"$engine/bin/morf" docs --io > "$scratch/io.md"
test "$(head -n 1 "$scratch/io.md")" = "# $reported I/O guide"
tail -n +4 "$scratch/io.md" | cmp - "$source_dir/docs/IO.md"
"$engine/bin/morf" test --no-dbus "$scratch/kit_spec.lua" "$scratch/frecency_spec.lua"
printf 'Installed engine and Lua library smoke checks passed\n'
