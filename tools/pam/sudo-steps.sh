#!/bin/sh
# sudo tools/pam/sudo-steps.sh [remove] -- markers in /etc/pam.d/sudo, so
# the shell can show which step sudo is on: the eyes while it looks for a
# face, a finger while it waits at the reader, a key at the password, a
# tick once it is through (lib.authsteps; caelestia's authsteps.lua).
#
# A marker (morf-auth-step, through pam_exec) goes before each step's
# lines -- before a pam_succeed_if that may skip the step, not between them,
# or it would be the marker that is skipped. Every marker is `optional`:
# it cannot stop sudo. The stack before is kept as /etc/pam.d/sudo.bak-steps;
# `remove` puts it back.
set -eu
[ "$(id -u)" = 0 ] || { echo "run it with sudo"; exit 1; }
STACK=/etc/pam.d/sudo
if [ "${1:-}" = remove ]; then
  [ -e $STACK.bak-steps ] && cp $STACK.bak-steps $STACK && echo "restored $STACK"
  rm -f /usr/local/bin/morf-auth-step
  exit 0
fi
here=$(cd "$(dirname "$0")" && pwd)
install -m 755 "$here/morf-auth-step" /usr/local/bin/morf-auth-step
echo "installed /usr/local/bin/morf-auth-step"

if grep -q morf-auth-step $STACK; then echo "$STACK has the markers already"; exit 0; fi
cp $STACK $STACK.bak-steps
M='optional     pam_exec.so quiet /usr/local/bin/morf-auth-step'
awk -v m="$M" '
  function mark(step) { print "auth       " m " " step }
  # A pam_succeed_if may skip the line after it: held, so the marker for
  # that line goes before both.
  /^auth[ \t].*pam_succeed_if/ { held = held $0 "\n"; next }
  /^auth[ \t].*(pam_gaze|pam_howdy)/ { mark("face") }
  /^auth[ \t].*pam_fprintd/ { mark("finger") }
  /^auth[ \t].*(include|substack)[ \t]+(system-auth|system-login|common-auth)/ { mark("password") }
  /^auth[ \t].*pam_unix/ { mark("password") }
  /^account[ \t]/ && !acc { print "account    " m " ok"; acc = 1 }
  { printf "%s", held; held = ""; print }
' $STACK.bak-steps > $STACK
echo "added the markers to $STACK (before: $STACK.bak-steps)"
