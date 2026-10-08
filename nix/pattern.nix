{ stdenv, libxcrypt, python3 }:
stdenv.mkDerivation {
  pname = "morf-pattern";
  version = "1";
  src = ../tools/pattern;
  buildInputs = [ libxcrypt ];
  buildPhase = ''
    $CC -std=c11 -O2 -Wall -Wextra -Werror -fstack-protector-strong -D_FORTIFY_SOURCE=2 \
      check.c -o morf-pattern-check -lcrypt
  '';
  installPhase = ''
    install -Dm755 morf-pattern-check "$out/libexec/morf-pattern-check"
    install -Dm755 morf-pattern "$out/bin/morf-pattern"
    substituteInPlace "$out/bin/morf-pattern" --replace-fail '#!/usr/bin/env python3' '#!${python3}/bin/python3 -I'
  '';
}
