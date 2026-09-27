#!/bin/sh
# tools/logre/update.sh -- rebuild logre (caelestia's greeter + lock, everything
# embedded) from ~/mold and install it as /usr/bin/logre.
#
# Run it as yourself, not with sudo: it builds as you and asks sudo only
# for the install. Safe to run again after every change.
set -eu

REPO=${REPO:-$HOME/mold}
MORF=${MORF:-$HOME/.local/bin/morf}
OUT=$REPO/target/dist/logre

[ "$(id -u)" != 0 ] || { echo "run it as yourself, not with sudo"; exit 1; }
[ -x "$MORF" ] || { echo "no morf at $MORF (make install first)"; exit 1; }

echo "== building $OUT"
cd "$REPO"
mkdir -p "$(dirname "$OUT")"
# The machine's own libraries, as outside a nix shell.
env -u LD_LIBRARY_PATH -u XDG_DATA_DIRS "$MORF" bundle examples/shells/caelestia/logre.lua -o "$OUT" \
  --with examples/shells/caelestia/greet \
  --with examples/shells/caelestia/lock \
  --with library/lib

echo "== installing /usr/bin/logre (sudo)"
# The logre that was there before the first update is kept, once.
if [ -e /usr/bin/logre ] && [ ! -e /usr/bin/logre.bak-old ]; then
  sudo cp /usr/bin/logre /usr/bin/logre.bak-old
  echo "   kept the old one as /usr/bin/logre.bak-old"
fi
sudo install -m 755 "$OUT" /usr/bin/logre

echo "== greetd"
CONF=/etc/greetd/config.toml
WANT='command = "cage -s -- /usr/bin/logre"'
if grep -qxF "$WANT" "$CONF"; then
  echo "   already runs /usr/bin/logre"
else
  sudo cp "$CONF" "$CONF.bak-logre-update"
  sudo sed -i "s|^command = .*|$WANT|" "$CONF"
  echo "   now runs /usr/bin/logre (old config kept as $CONF.bak-logre-update)"
fi

echo
echo "done."
echo "  lock:    Super+L  (logre -- lock)"
echo "  greeter: at your next logout; if it does not come up, Ctrl+Alt+F2 and"
echo "           sudo cp /usr/bin/logre.bak-old /usr/bin/logre"
