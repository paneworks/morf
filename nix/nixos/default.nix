{ flake }:
{
  config,
  lib,
  pkgs,
  options,
  ...
}:
let
  morf = config.programs.morf.package;
  user = config.programs.morf.user;
  userHome = config.users.users.${user}.home;
  greeterFonts = with pkgs; [
    roboto
    material-symbols
    ibm-plex
    nerd-fonts."m+"
  ];
  fontFiles = [
    "${pkgs.roboto}/share/fonts/truetype/Roboto-Regular.ttf"
    "${pkgs.material-symbols}/share/fonts/truetype/MaterialSymbolsRounded[FILL,GRAD,opsz,wght].ttf"
    "${pkgs.ibm-plex}/share/fonts/opentype/IBMPlexMono-Regular.otf"
    "${pkgs.nerd-fonts."m+"}/share/fonts/truetype/NerdFonts/M+/M+1NerdFont-Regular.ttf"
  ];
  caelestiaConfig = pkgs.runCommand "morf-caelestia" { } ''
    mkdir -p "$out"
    cp -R ${flake}/examples/shells/caelestia/. "$out/"
    chmod -R u+w "$out"
    # Ship the complete Lua library from the same source as the theme. The
    # executable may still come from Cachix; this step only copies files.
    mkdir -p "$out/library"
    cp -R ${flake}/library/lib "$out/library/"
    mkdir -p "$out/fonts"
    for part in shell lock greet; do
      if [ -d "$out/$part/fonts" ]; then
        cp -R "$out/$part/fonts/." "$out/fonts/"
        rm -rf "$out/$part/fonts"
      fi
      cat > "$out/$part/appearance-default.json" <<'EOF'
    { "theme": "tsugumori", "font": "" }
    EOF
      ln -s ../fonts "$out/$part/fonts"
    done
    ${lib.concatMapStringsSep "\n" (font: ''
      test -f ${lib.escapeShellArg font}
      ln -s ${lib.escapeShellArg font} "$out/fonts/"
    '') fontFiles}
  '';
  # Keep advanced CLI invocations unchanged; protect the default entry points.
  morfDefault = pkgs.writeShellScriptBin "morf" ''
    set -u
    export MORF_RUNTIME_PATH="${
      lib.concatMapStringsSep ":" toString config.programs.morf.runtimePaths
    }''${MORF_RUNTIME_PATH:+:$MORF_RUNTIME_PATH}"
    export CAELESTIA_SCALE_MODE=compositor
    export CAELESTIA_SCALE_FILE="''${CAELESTIA_SCALE_FILE:-${config.programs.morf.uiScaleFile}}"
    export CAELESTIA_SCALE_DEFAULT="''${CAELESTIA_SCALE_DEFAULT:-${toString config.programs.morf.uiScale}}"
    if [ "$#" -gt 1 ]; then exec ${morf}/bin/morf "$@"; fi
    role="''${1:-shell}"
    case "$role" in shell|lock|greet) ;; *) exec ${morf}/bin/morf "$@" ;; esac
    if [ "$role" = shell ] && [ -n "''${MORF_CONFIG:-}" ]; then
      exec ${morf}/bin/morf "$@"
    fi
    root="''${XDG_CONFIG_HOME:-$HOME/.config}"
    if [ "$role" = greet ] && [ -n "''${MORF_GREETER_CONFIG_HOME:-}" ]; then
      root="$MORF_GREETER_CONFIG_HOME"
    fi
    candidate="$root/morf/default/$role/init.lua"
    if [ "$role" = shell ] && [ -r "$root/morf/shell.lua" ]; then candidate="$root/morf/shell.lua"; fi
    fallback="/etc/xdg/morf/default/$role/init.lua"
    if [ -r "$candidate" ]; then
      if [ "$role" = greet ] && [ -n "''${MORF_GREETER_CONFIG_HOME:-}" ]; then
        theme=$(${pkgs.coreutils}/bin/dirname "$(${pkgs.coreutils}/bin/dirname "$(${pkgs.coreutils}/bin/readlink -f "$candidate")")")
        if [ -r "$theme/appearance.json" ]; then export CAELESTIA_APPEARANCE="$theme/appearance.json"; fi
      fi
      check_args=()
      if [ "$role" = lock ]; then check_args=(-- window preview); fi
      if CAELESTIA_DRY_RUN=1 ${pkgs.coreutils}/bin/timeout 10 \
        ${morf}/bin/morf check "$candidate" --no-dbus --isolate --after 0 "''${check_args[@]}" >/dev/null; then
        ${morf}/bin/morf "$candidate"
        status=$?
        if [ "$status" -eq 0 ]; then exit 0; fi
        echo "Morf $role exited with $status; using $fallback" >&2
      else
        echo "Morf $role configuration failed validation; using $fallback" >&2
      fi
    fi
    if [ "$role" = greet ] && [ -n "''${MORF_GREETER_CONFIG_HOME:-}" ]; then unset CAELESTIA_APPEARANCE; fi
    export XDG_CONFIG_DIRS=/etc/xdg
    exec ${morf}/bin/morf "$fallback"
  '';
  greeterAccess = pkgs.writeShellScript "morf-greeter-access" ''
    # No write access, and no directory listing outside the Morf tree.
    home=$(${pkgs.coreutils}/bin/readlink -e ${lib.escapeShellArg userHome}) || exit 0
    root=$(${pkgs.coreutils}/bin/readlink -e "$home/.config/morf") || exit 0
    [ -d "$root" ] || exit 0
    for start in "$home" "$(${pkgs.coreutils}/bin/readlink -e "$home/.config")" "$root"; do
      parent="$start"
      while [ -n "$parent" ]; do
        case "$parent" in "$home"|"$home"/*) ;; *) break ;; esac
        ${pkgs.acl}/bin/setfacl -m u:greeter:--x "$parent" || echo "Cannot grant greeter traversal on $parent" >&2
        parent=$(${pkgs.coreutils}/bin/dirname "$parent")
      done
    done
    case "$root" in
      "$home"/*)
        ${pkgs.acl}/bin/setfacl -R -P -m u:greeter:rX "$root" || echo "Cannot grant greeter read access on $root" >&2
        ${pkgs.findutils}/bin/find "$root" -type d -exec ${pkgs.acl}/bin/setfacl -m d:u:greeter:r-x {} + \
          || echo "Cannot set inherited greeter read access on $root" >&2
        ;;
    esac
    exit 0
  '';
  greeter = pkgs.writeShellScript "morf-greeter" ''
    export PATH=${
      lib.makeBinPath [
        pkgs.systemd
        pkgs.fontconfig
      ]
    }:/run/current-system/sw/bin
    export XDG_CONFIG_HOME=/var/lib/greetd/.config
    export MORF_GREETER_CONFIG_HOME=${lib.escapeShellArg "${userHome}/.config"}
    export XDG_CONFIG_DIRS=${lib.escapeShellArg "${userHome}/.config:/etc/xdg"}
    export XDG_DATA_DIRS=${config.services.displayManager.sessionData.desktops}/share:/run/current-system/sw/share
    exec ${config.system.build.morfGreeterCompositor}
  '';
  scaleDirectory = "/var/lib/morf/ui-scale/${user}";
  directory = "/var/lib/morf/wallpaper/${user}";
  lule = config.programs.morf.lulePackage;
  bridge = pkgs.writeShellScriptBin "morf-wallpaper" ''
    exec ${pkgs.python3}/bin/python3 ${./scripts/wallpaper.py} "$@"
  '';
  # A minimal greeter-only palette config: user hooks belong to their session.
  luleConfig = pkgs.writeTextDir "init.lua" ''
    local lule = require("lule")
    lule.theme = "dark"
    lule.palette = "pigment"
  '';
  fallbackLogo = pkgs.writeText "morf-wallpaper-logo.svg" ''
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
      <path fill="#ffffff" d="M15 75V25h14l21 25 21-25h14v50H70V48L50 71 30 48v27z"/>
    </svg>
  '';
in
{
  imports = [
    ./compositor.nix
    ./idle.nix
    ./phone.nix
  ];

  options.programs.morf = {
    enable = lib.mkEnableOption "Morf desktop, greeter and lockscreen integration";
    user = lib.mkOption {
      type = lib.types.str;
      description = "Primary desktop user whose preferences are shared with the greeter.";
    };
    package = lib.mkOption {
      type = lib.types.package;
      default = flake.packages.${pkgs.stdenv.hostPlatform.system}.morf;
      description = "Morf executable; a cached package can be supplied independently of the theme.";
    };
    libraryPackage = lib.mkOption {
      type = lib.types.package;
      default =
        config.programs.morf.package.library
          or flake.packages.${pkgs.stdenv.hostPlatform.system}.morf-library;
      description = "Installed Lua library matching the selected executable.";
    };
    lulePackage = lib.mkOption {
      type = lib.types.package;
      description = "Lule package used to generate and share the session wallpaper.";
    };
    wallpaperLogo = lib.mkOption {
      type = lib.types.str;
      default = "${userHome}/.config/morf/logo.svg";
      description = "Optional logo for generated wallpapers.";
    };
  };
  options.programs.morf.uiScale = lib.mkOption {
    type = lib.types.addCheck lib.types.number (value: value >= 0.5 && value <= 2);
    default = 1.0;
    description = "Initial Hyprland display scale shared by desktop, lock and greet. Morf's scale slider persists its override.";
  };
  options.programs.morf.uiScaleFile = lib.mkOption {
    type = lib.types.str;
    readOnly = true;
    default = "${scaleDirectory}/scale.json";
    description = "Shared, user-writable UI scale preference readable by the greeter.";
  };
  options.programs.morf.runtimePaths = lib.mkOption {
    type = lib.types.listOf lib.types.path;
    default = [ "${flake}/library" ];
    description = "Additional Morf desktop modules and startup plugins.";
  };

  config = lib.mkIf config.programs.morf.enable (
    {
      programs.hyprland.enable = lib.mkDefault true;
      services.greetd = {
        enable = true;
        settings.default_session = {
          command = "${greeter}";
          user = "greeter";
        };
      };

      users.users.greeter = {
        home = "/var/lib/greetd";
        createHome = true;
      };

      environment.systemPackages = [
        (lib.hiPrio morfDefault)
        config.programs.morf.libraryPackage
        bridge
      ];
      system.build.morfLauncher = morfDefault;
      system.build.morfGreeter = greeter;
      system.build.morfGreeterAccess = greeterAccess;
      # Account activation reapplies the private home mode, which clears the ACL
      # mask. Regrant traversal even when the Home Manager package did not change.
      system.activationScripts.morfGreeterAccess = {
        deps = [ "users" ];
        text = "${greeterAccess}";
      };
      systemd.services.morf-greeter-access = {
        description = "Allow the greeter to read the primary user's Morf theme";
        wantedBy = [ "multi-user.target" ];
        after = [ "home-manager-${user}.service" ];
        before = [ "greetd.service" ];
        restartTriggers = lib.optionals (options ? home-manager) [
          config.home-manager.users.${user}.home.activationPackage
        ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          ExecStart = greeterAccess;
        };
      };
      systemd.services.greetd.wants = [ "morf-greeter-access.service" ];

      environment.etc."xdg/morf/caelestia".source = caelestiaConfig;
      environment.etc."xdg/morf/default".source = caelestiaConfig;
      environment.etc."greetd/default-session" = lib.mkIf config.programs.hyprland.enable {
        text = if config.programs.hyprland.withUWSM then "hyprland-uwsm\n" else "hyprland\n";
      };
      programs.hyprland.withUWSM = lib.mkIf config.programs.hyprland.enable (lib.mkDefault true);

      systemd.user.services.morf = {
        description = "Morf desktop shell";
        wantedBy = [ "graphical-session.target" ];
        after = [ "graphical-session.target" ];
        partOf = [ "graphical-session.target" ];
        path = [ "/run/current-system/sw" ];
        environment.MORF_RUNTIME_PATH =
          lib.concatMapStringsSep ":" toString
            config.programs.morf.runtimePaths;
        serviceConfig = {
          ExecStartPre = "-${bridge}/bin/morf-wallpaper adopt";
          ExecStart = "${morfDefault}/bin/morf shell";
          ExecStopPost = "-${bridge}/bin/morf-wallpaper publish";
          Restart = "on-failure";
          RestartSec = 2;
        };
      };
      security.pam.services.morf-lock.fprintAuth = false;
      security.pam.services.greetd.fprintAuth = false;
      security.pam.services.morf-lock-finger = lib.mkIf config.services.fprintd.enable {
        text = ''
          auth sufficient ${config.services.fprintd.package}/lib/security/pam_fprintd.so
          auth required ${pkgs.pam}/lib/security/pam_deny.so
          account include morf-lock
        '';
      };
      fonts.packages = greeterFonts;

      system.build.morfWallpaper = bridge;
      environment.etc."morf/wallpaper.json".text = builtins.toJSON {
        inherit user directory;
        logo = config.programs.morf.wallpaperLogo;
        fallback_logo = toString fallbackLogo;
        lule = "${lule}/bin/lule";
        lule_config = toString luleConfig;
        command = "${bridge}/bin/morf-wallpaper";
      };
      users.groups.morf-wallpaper.members = [
        user
        "greeter"
      ];
      systemd.tmpfiles.rules = [
        "d /var/lib/morf 0755 root root -"
        "d /var/lib/morf/ui-scale 0755 root root -"
        "d ${scaleDirectory} 0755 ${user} ${config.users.users.${user}.group} -"
        "d /var/lib/morf/wallpaper 0755 root root -"
        "d ${directory} 2770 ${user} morf-wallpaper -"
        "f ${directory}/.lock 0666 ${user} morf-wallpaper -"
      ];
      systemd.services.morf-wallpaper-logo = {
        description = "Share the wallpaper logo with the greeter";
        wantedBy = [ "multi-user.target" ];
        after = [
          "systemd-tmpfiles-setup.service"
          "home-manager-${user}.service"
        ];
        before = [ "morf-wallpaper-boot.service" ];
        serviceConfig = {
          Type = "oneshot";
          User = user;
          ExecStart = "${bridge}/bin/morf-wallpaper logo";
          RemainAfterExit = true;
        };
      };
      systemd.services.morf-wallpaper-boot = {
        description = "Generate the shared Lule wallpaper once per boot";
        wantedBy = [ "multi-user.target" ];
        wants = [ "morf-wallpaper-logo.service" ];
        after = [
          "systemd-tmpfiles-setup.service"
          "morf-wallpaper-logo.service"
        ];
        before = [ "greetd.service" ];
        serviceConfig = {
          Type = "oneshot";
          User = "greeter";
          # Lule's palette extractor uses /tmp/lule_palette. Keep the greeter's
          # scratch file separate from the one owned by the logged-in user.
          PrivateTmp = true;
          ExecStart = "${bridge}/bin/morf-wallpaper greet";
          RemainAfterExit = true;
          TimeoutStartSec = 250;
        };
      };
    }
    // lib.optionalAttrs (options ? home-manager) {
      home-manager.users.${user} = { lib, ... }: {
        home.activation.restartMorf = lib.hm.dag.entryAfter [ "reloadSystemd" ] ''
          if ${pkgs.systemd}/bin/systemctl --user is-active --quiet morf.service 2>/dev/null; then
            run ${pkgs.systemd}/bin/systemctl --user restart morf.service
          fi
        '';
      };

    }
  );
}
