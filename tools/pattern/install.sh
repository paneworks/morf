#!/bin/sh
# sudo tools/pattern/install.sh -- the pattern for the lock and the greeter,
# and for nothing else (sudo, polkit, a tty login keep the password alone).
#
#   /usr/local/bin/morf-pattern-check    what pam_exec runs
#   /usr/local/bin/morf-pattern          set | clear | status
#   /etc/pam.d/morf-lock                 the lock's stack (the lock uses it
#                                        when it is there)
#   /etc/pam.d/greetd                    one line added, before its include
#
# Each stack tries the input as a pattern first and, when it is not one,
# as the password: one field takes either, and a wrong pattern counts
# toward faillock like a wrong password. Without a pattern set nothing
# changes. Files replaced are kept as *.bak-pattern.
set -eu
[ "$(id -u)" = 0 ] || { echo "run it with sudo"; exit 1; }
here=$(cd "$(dirname "$0")" && pwd)
install -m 0755 "$here/morf-pattern-check" /usr/local/bin/morf-pattern-check
install -m 0755 "$here/morf-pattern" /usr/local/bin/morf-pattern
LINE='auth       sufficient   pam_exec.so quiet expose_authtok /usr/local/bin/morf-pattern-check'

[ -e /etc/pam.d/morf-lock ] && cp /etc/pam.d/morf-lock /etc/pam.d/morf-lock.bak-pattern
cat > /etc/pam.d/morf-lock <<PAM
#%PAM-1.0
# The morf lock screen: a pattern (morf-pattern) or the password.
$LINE
auth       include      system-auth
account    include      system-auth
PAM
echo "wrote /etc/pam.d/morf-lock"

if [ -e /etc/pam.d/greetd ] && ! grep -q morf-pattern-check /etc/pam.d/greetd; then
  cp /etc/pam.d/greetd /etc/pam.d/greetd.bak-pattern
  awk -v line="$LINE" '!done && /^auth[ \t]+include/ { print line; done = 1 } { print }' \
    /etc/pam.d/greetd.bak-pattern > /etc/pam.d/greetd
  echo "added the pattern to /etc/pam.d/greetd"
fi
echo "now: sudo morf-pattern set $(logname 2>/dev/null || echo USER)"
