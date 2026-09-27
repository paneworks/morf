#!/bin/sh
# sudo tools/pam/readers.sh -- the stacks the lock listens to for a finger
# and a face, beside the password (and pattern) stack:
#
#   /etc/pam.d/morf-lock-finger   pam_fprintd -- listened to at rest too
#   /etc/pam.d/morf-lock-face     pam_gaze    -- while the sheet is up
#
# Each is its module and pam_deny after it: a finger or a face that does
# not match ends there, never at a password prompt, so it is never counted
# as a failed login by pam_faillock. Only the lock reads these; sudo, polkit
# and logins keep their own stacks. `sudo tools/pam/readers.sh remove` takes
# them away again.
set -eu
[ "$(id -u)" = 0 ] || { echo "run it with sudo"; exit 1; }
if [ "${1:-}" = remove ]; then
  rm -f /etc/pam.d/morf-lock-finger /etc/pam.d/morf-lock-face
  echo "removed"; exit 0
fi
lib=/usr/lib/security
if [ -e "$lib/pam_fprintd.so" ]; then
  cat > /etc/pam.d/morf-lock-finger <<PAM
#%PAM-1.0
# The morf lock's fingerprint: the reader alone, nothing to fall through to.
auth       sufficient   pam_fprintd.so
auth       required     pam_deny.so
account    include      system-auth
PAM
  echo "wrote /etc/pam.d/morf-lock-finger"
else
  echo "no pam_fprintd.so: no fingerprint stack"
fi
face=""
for m in pam_gaze.so pam_howdy.so; do [ -e "$lib/$m" ] && { face=$m; break; }; done
if [ -n "$face" ]; then
  cat > /etc/pam.d/morf-lock-face <<PAM
#%PAM-1.0
# The morf lock's face: the camera alone, nothing to fall through to.
auth       sufficient   $face
auth       required     pam_deny.so
account    include      system-auth
PAM
  echo "wrote /etc/pam.d/morf-lock-face ($face)"
else
  echo "no pam_gaze.so or pam_howdy.so: no face stack"
fi
