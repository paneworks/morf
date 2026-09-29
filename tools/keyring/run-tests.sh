#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
env -u LD_LIBRARY_PATH -u PYTHONPATH dbus-run-session -- /usr/bin/python3 tools/keyring/test_prompter.py
env -u LD_LIBRARY_PATH -u PYTHONPATH TEST_GCR_VERSION=3 dbus-run-session -- /usr/bin/python3 tools/keyring/test_prompter.py
env -u LD_LIBRARY_PATH -u PYTHONPATH dbus-run-session -- /usr/bin/python3 tools/keyring/test_ui.py
