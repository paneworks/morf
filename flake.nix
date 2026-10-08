{
  description = "morf Rust development shell";

  nixConfig = {
    extra-substituters = [ "https://paneworks.cachix.org" ];
    extra-trusted-public-keys = [ "paneworks.cachix.org-1:5XAOHaQHgDEM4dL1Cpu56zcKZxUWYP7zmv8GD3Siy0Q=" ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?rev=4c1018dae018162ec878d42fec712642d214fdfa";
    flake-utils.url = "github:numtide/flake-utils";
    nixgl.url = "github:nix-community/nixGL";
    # A rust toolchain that can be asked for another target's standard library.
    # The one in nixpkgs carries the host's and only the host's, which is enough
    # until the day something has to run on a phone.
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    { self, nixpkgs, flake-utils, nixgl, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [
          rust-overlay.overlays.default
          (final: prev: {
            xorg = prev.xorg // {
              libX11 = final.libx11;
              libxcb = final.libxcb;
              libxshmfence = final.libxshmfence;
            };
          })
        ];

        pkgs = import nixpkgs {
          inherit system overlays;
          config = {
            allowUnfree = true;
            nvidia.acceptLicense = true;
          };
        };

        nvidiaVersion = builtins.getEnv "NVIDIA_VERSION";
        hasNvidia = nvidiaVersion != "";

        nixglPkgs = import "${nixgl}/default.nix" ({
          inherit pkgs;
        } // pkgs.lib.optionalAttrs hasNvidia {
          inherit nvidiaVersion;
          nvidiaHash = null;
        });

        nixGLTarget =
          if hasNvidia
          then "${nixglPkgs.nixGLNvidia}/bin/nixGLNvidia-${nvidiaVersion}"
          else "${nixglPkgs.nixGLIntel}/bin/nixGLIntel";
        nixVulkanTarget =
          if hasNvidia
          then "${nixglPkgs.nixVulkanNvidia}/bin/nixVulkanNvidia-${nvidiaVersion}"
          else "${nixglPkgs.nixVulkanIntel}/bin/nixVulkanIntel";

        nixGLAlias = pkgs.runCommand "nixGL" { } ''
          mkdir -p $out/bin
          ln -s ${nixGLTarget} $out/bin/nixGL
        '';
        nixVulkanAlias = pkgs.runCommand "nixVulkan" { } ''
          mkdir -p $out/bin
          ln -s ${nixVulkanTarget} $out/bin/nixVulkan
        '';

        # postmarketOS is Alpine underneath, so the target is musl and not glibc.
        # Getting that wrong produces a binary that links, ships, and then dies
        # on the device looking for an interpreter that was never there.
        crossTriple = "aarch64-unknown-linux-musl";
        crossPkgs = import nixpkgs {
          inherit system overlays;
          crossSystem.config = crossTriple;
        };
        crossCc = "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}cc";
        # The two libraries the engine actually links against. Vulkan is not one
        # of them — wgpu opens the loader at run time, so the device's own
        # driver is found on the device and nothing is needed here.
        crossLibs = [ crossPkgs.wayland crossPkgs.libxkbcommon ];

        guiLibs = with pkgs; [
          # Linux-PAM, so authentication works from this shell at all. A binary
          # built against Nix's glibc uses Nix's loader, and that loader never
          # searches /usr/lib: the system's libpam resolves by path and then
          # cannot find libaudit beside it. This one brings its own chain, and
          # its own modules under lib/security, which is where a service file
          # naming `pam_exec.so` will be looked up.
          pam
          # libpipewire, for `morf.audio`. Opened at run time like the Vulkan
          # loader, never linked, so only the library is needed -- no headers,
          # no bindgen -- and only on the search path: a binary built here
          # uses Nix's loader, which does not look in /usr/lib for it.
          pipewire
          alsa-lib
          udev
          vulkan-loader
          libxkbcommon
          wayland
          libx11
          libxcursor
          libxi
          libxrandr
        ];
        # The libraries morf opens at run time rather than links: the Vulkan
        # loader and EGL (wgpu), PipeWire (`morf.audio`), PAM (the lock
        # screen and greeter), udev. A binary built by Nix uses Nix's loader,
        # which never looks in /usr/lib, so the wrapper puts them on its path.
        # libwayland-client and libxkbcommon are linked, and also listed so a
        # dlopen of either finds the same copy.
        runtimeLibs = with pkgs; [
          vulkan-loader
          libglvnd
          pipewire
          pam
          udev
          wayland
          libxkbcommon
        ];

        # The same toolchain as the shell: the code is edition 2024 with
        # let-chains, newer than the pinned nixpkgs' rustc.
        rustPlatform = pkgs.makeRustPlatform {
          cargo = pkgs.rust-bin.stable.latest.minimal;
          rustc = pkgs.rust-bin.stable.latest.minimal;
        };

        # `nix build` -- the `morf` binary, and the Lua library it ships with
        # under share/morf/library, where morf finds it through XDG_DATA_DIRS
        # (a NixOS system profile and a user profile both put their share/
        # there; the wrapper adds this package's own as well).
        morf = rustPlatform.buildRustPackage {
          pname = "morf";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          outputs = [ "out" "library" ];
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            # Build outputs and research notes are no part of the source.
            filter = path: _type:
              let name = baseNameOf path; in
              name != "target" && name != "xtra" && name != "result";
          };
          cargoLock = {
            lockFile = ./Cargo.lock;
            # luna comes from git; Cargo.lock pins its revision.
            allowBuiltinFetchGit = true;
          };
          cargoBuildFlags = [ "--package" "morf-cli" ];
          nativeBuildInputs = with pkgs; [ pkg-config makeWrapper ];
          buildInputs = with pkgs; [ wayland libxkbcommon ];
          doCheck = false;
          postInstall = ''
            mkdir -p $library/share/morf/library $out/share/morf
            cp -r library/lib library/README.md library/luarc.template.json $library/share/morf/library/
            substituteInPlace $library/share/morf/library/luarc.template.json \
              --replace-fail '~/.local/share/morf/library' "$library/share/morf/library"
            $out/bin/morf types $library/share/morf/library/types
            ln -s $library/share/morf/library $out/share/morf/library
            wrapProgram $out/bin/morf \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs} \
              --suffix XDG_DATA_DIRS : $out/share
          '';
          doInstallCheck = true;
          installCheckPhase = ''
            runHook preInstallCheck
            bash tools/nix-smoke.sh "$out" "$library"
            runHook postInstallCheck
          '';
          meta = {
            description = "Rendering and shell engine in Rust, configured in Lua";
            homepage = "https://github.com/paneworks/morf";
            license = pkgs.lib.licenses.mit;
            mainProgram = "morf";
            platforms = pkgs.lib.platforms.linux;
            outputsToInstall = [ "out" ];
          };
        };
      in
      {
        # `nix develop .#cross-aarch64` — then `cargo build --release
        # --target aarch64-unknown-linux-musl`, and the binary runs on the
        # phone. Cross-compiling means building the target's libraries from
        # source the first time, because there is no binary cache for them;
        # after that it is as quick as any other build here, and quicker than
        # the device managing it itself.
        devShells.cross-aarch64 = pkgs.mkShell {
          packages = [
            (pkgs.rust-bin.stable.latest.default.override {
              targets = [ crossTriple ];
            })
            pkgs.pkg-config
            crossPkgs.stdenv.cc
          ];

          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = crossCc;
          CC_aarch64_unknown_linux_musl = crossCc;
          # Dynamic, not static. A static musl binary cannot load the device's
          # own `libwayland-client`, and that is the whole point of the exercise.
          #
          # And pointed at the device's loader by absolute path. Without this the
          # binary asks for the musl that built it — a `/nix/store` path that
          # exists on this machine and nowhere else — and the device answers
          # "no such file or directory" about a file that is plainly there,
          # because the file it cannot find is the interpreter and not the
          # binary.
          CARGO_BUILD_RUSTFLAGS =
            "-C target-feature=-crt-static "
            + "-C link-arg=-Wl,--dynamic-linker=/lib/ld-musl-aarch64.so.1";
          PKG_CONFIG_ALLOW_CROSS = "1";
          # Only the cross libraries, so pkg-config cannot find this machine's
          # own and hand back an x86 path that links and then does not run.
          PKG_CONFIG_LIBDIR =
            pkgs.lib.concatStringsSep ":"
              (map (lib: "${lib.dev}/lib/pkgconfig") crossLibs);
          # And emptied, because `PKG_CONFIG_PATH` is searched *as well as*
          # `PKG_CONFIG_LIBDIR`, not instead of it. Whatever loaded the ordinary
          # shell — direnv, usually — leaves this machine's own `.pc` files on
          # it, and the linker then finds an x86 library, says it is skipping
          # something incompatible, and fails having never looked anywhere else.
          PKG_CONFIG_PATH = "";
          # Host libraries have no business on a cross build's search path.
          LD_LIBRARY_PATH = "";
        };

        devShells.default = pkgs.mkShell {
          packages = [
            # rust-overlay's stable rather than nixpkgs' rustc: the native Lua
            # tier (luna's `jit` feature, Cranelift) needs a newer compiler
            # than the pinned nixpkgs carries.
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
            })
            pkgs.git-cliff
            pkgs.clang
            pkgs.pkg-config

            nixGLAlias
            nixVulkanAlias
            nixglPkgs.nixGLIntel
            nixglPkgs.nixVulkanIntel
          ] ++ pkgs.lib.optionals hasNvidia [
            nixglPkgs.nixGLNvidia
            nixglPkgs.nixVulkanNvidia
          ] ++ guiLibs;

          # A musl toolchain for the static build, handed over as a path rather than a package.
          # As a package its headers land on the default search path, and an ordinary build then
          # compiles against musl while linking against glibc -- which succeeds without a word and
          # crashes at startup. Only the static build is given it: .make.lua reads MUSL_CC.
          # gcc targeting musl, which is the only one of the two that has a C++ standard library.
          MUSL_CC = pkgs.pkgsMusl.stdenv.cc;
          # musl-clang: the host clang, pointed at musl's headers and libs. C only -- it has no
          # libstdc++, so a C++ build against it fails on the first #include <string>.
          MUSL_CLANG = pkgs.musl.dev;

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath guiLibs;
          WGPU_VALIDATION = "0";
          WGPU_DEBUG = "0";
        };
      } // (if builtins.elem system [ "x86_64-linux" "aarch64-linux" ] then {
        packages = {
          default = morf;
          inherit morf;
          morf-library = morf.library;
        };
        apps.default = flake-utils.lib.mkApp { drv = morf; };
        apps.morf = flake-utils.lib.mkApp { drv = morf; };
        checks.morf = morf;
      } else {})
    ) // {
      nixosModules.default = import ./nix/nixos { flake = self; };
      nixosModules.morf = self.nixosModules.default;
    };
}
