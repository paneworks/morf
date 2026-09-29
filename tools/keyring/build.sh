#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
mkdir -p target/dist
# Use one coherent native toolchain, not headers/libraries from an inherited Nix shell.
env -u LD_LIBRARY_PATH -u LIBRARY_PATH -u CPATH -u C_INCLUDE_PATH -u COMPILER_PATH -u GCC_EXEC_PREFIX PATH=/usr/bin:/bin /usr/bin/cc \
  -std=gnu11 -O2 -Wall -Wextra -Werror tools/keyring/morf-keyring.c \
  $(env -u PKG_CONFIG_PATH -u PKG_CONFIG_LIBDIR /usr/bin/pkg-config --cflags --libs gcr-4 gio-unix-2.0 json-glib-1.0) \
  -o target/dist/morf-keyring
