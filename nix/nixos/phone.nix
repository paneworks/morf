{
  config,
  lib,
  pkgs,
  ...
}:
let
  pointerConfig = ''
    hl.config({ cursor = { invisible = true } })
  '';
  workspaceConfig = ''
    -- Morf owns bottom-edge contact sequences, including workspace previews.
    hl.config({ gestures = {
      workspace_swipe_touch = false,
    } })
    -- The native preview has already finished the transition at handoff.
    hl.animation({ leaf = "workspaces", enabled = ${lib.boolToString (config.programs.morf.phone.gestureDriver == "lisgd")}, speed = 4, bezier = "default", style = "slide" })
  '';

  hyprland = config.programs.hyprland.package;
  dpms = pkgs.writeShellScriptBin "phone-dpms" ''
    exec ${config.system.build.hyprlandIdleDpms} "$@"
  '';
  screen = pkgs.writeShellApplication {
    name = "phone-screen";
    runtimeInputs = [
      hyprland
      dpms
      pkgs.coreutils
      pkgs.gawk
      pkgs.jq
      pkgs.systemd
      pkgs.util-linux
    ];
    text = builtins.readFile ./scripts/screen.sh;
  };
  deliberateWakeConfig = ''
    -- Pocket touches must not wake the screen. The power key is handled
    -- by logind in the user session and by the explicit greeter binding.
    hl.config({ misc = {
      key_press_enables_dpms = false,
      mouse_move_enables_dpms = false,
    } })
  '';
  makeIdleConfig =
    timeout:
    pkgs.writeText "phone-hypridle.conf" ''
      general {
        lock_cmd = ${screen}/bin/phone-screen toggle
        before_sleep_cmd = ${screen}/bin/phone-screen off
        inhibit_sleep = 3
      }
      ${lib.optionalString (timeout > 0) ''
        listener {
          timeout = ${toString timeout}
          on-timeout = ${screen}/bin/phone-screen off
        }
      ''}
    '';
  idleConfig = makeIdleConfig config.programs.morf.phone.idleTimeout;
  greeterIdleConfig = makeIdleConfig 60;
  gestureAction = pkgs.writeShellApplication {
    name = "phone-gesture";
    runtimeInputs = [
      config.programs.hyprland.package
      config.programs.morf.package
      pkgs.jq
      pkgs.coreutils
    ];
    text = ''
      case "''${1:-}" in
        workspace-next|workspace-previous|dashboard|top|keyboard) ;;
        *) exit 2 ;;
      esac
      # Let Wayland deliver the final contacts before querying Morf's context.
      sleep 0.05
      # Global input must never act on the desktop behind a session lock.
      hyprctl -j locked | jq -e '.locked == false' >/dev/null || exit 0
      timeout 3 morf -i "$WAYLAND_DISPLAY" ipc call phone-gesture "$1" >/dev/null
    '';
  };
  gestureRunner = pkgs.writeShellApplication {
    name = "phone-gestures";
    runtimeInputs = [
      pkgs.lisgd
      config.programs.hyprland.package
      pkgs.jq
      pkgs.systemd
      pkgs.coreutils
      pkgs.gnugrep
    ];
    text = ''
      export PHONE_GESTURE_ACTION=${gestureAction}/bin/phone-gesture
      ${builtins.readFile ./scripts/gestures.sh}
    '';
  };
in
{
  options.programs.morf.phone.enable =
    lib.mkEnableOption "Morf phone screen power and touch gestures";
  options.programs.morf.phone.idleTimeout = lib.mkOption {
    type = lib.types.ints.unsigned;
    default = 60;
    description = "Seconds before locking and blanking a phone session; zero disables automatic locking.";
  };
  options.programs.morf.phone.gestureDriver = lib.mkOption {
    type = lib.types.enum [ "native" "lisgd" ];
    default = "native";
    description = ''
      Native gestures follow the finger and settle with its release velocity.
      lisgd is a completed-swipe fallback for older Morf engines.
    '';
  };
  config =
    lib.mkIf
      (
        config.programs.morf.enable && config.programs.morf.phone.enable && config.programs.hyprland.enable
      )
      {
        services.logind.settings.Login.HandlePowerKey = "lock";
        users.users.${config.programs.morf.user}.extraGroups = [ "input" ];
        environment.systemPackages = [ screen ]
          ++ lib.optional (config.programs.morf.phone.gestureDriver == "lisgd") pkgs.lisgd;
        # Shared by the user's lockscreen and the separate greeter process.
        environment.etc."morf/phone-screen.json".text = builtins.toJSON {
          command = "${screen}/bin/phone-screen";
          doubleTap = true;
        };
        environment.etc."xdg/hypr/hypridle.conf".source = lib.mkForce idleConfig;
        environment.etc."xdg/hypr/hypridle.conf".text = lib.mkForce null;
        programs.morf.hyprland.extraConfig = deliberateWakeConfig + pointerConfig + workspaceConfig;
        programs.morf.greeter = {
          environment.MORF_PHONE_GREETER = "1";
          sessionSetup = ''
            ${config.services.hypridle.package}/bin/hypridle --config ${greeterIdleConfig} &
            idle=$!
            trap 'kill "$idle" 2>/dev/null || true' EXIT
          '';
          extraConfig =
            deliberateWakeConfig
            + pointerConfig
            + ''
              -- logind does not send Lock to sessions of class greeter.
              hl.bind("XF86PowerOff", hl.dsp.exec_cmd("${screen}/bin/phone-screen toggle"), { locked = true })
            '';
        };
        systemd.user.services.morf.environment.CAELESTIA_GESTURE_DRIVER = config.programs.morf.phone.gestureDriver;
        systemd.user.services.phone-gestures = lib.mkIf (config.programs.morf.phone.gestureDriver == "lisgd") {
          description = "Phone edge swipes through lisgd";
          wantedBy = [ "graphical-session.target" ];
          after = [
            "graphical-session.target"
            "morf.service"
          ];
          partOf = [ "graphical-session.target" ];
          serviceConfig = {
            ExecStart = "${gestureRunner}/bin/phone-gestures";
            Restart = "on-failure";
            RestartSec = 2;
            TimeoutStopSec = 5;
          };
        };
        # Keep phone preferences writable and stable when the theme path changes.
        # Seed the visible bar once; Caelestia can save other preferences normally.
        systemd.user.services.morf = {
          environment.CAELESTIA_SETTINGS = "%h/.local/state/caelestia/phone.json";
          preStart = ''
            ${pkgs.coreutils}/bin/mkdir -p "$(${pkgs.coreutils}/bin/dirname "$CAELESTIA_SETTINGS")"
            if [ ! -e "$CAELESTIA_SETTINGS" ]; then
              ${pkgs.coreutils}/bin/install -m 600 ${
                pkgs.writeText "caelestia-phone.json" (
                  builtins.toJSON {
                    edgebar.enabled = "on";
                  }
                )
              } "$CAELESTIA_SETTINGS"
            fi
          '';
        };
      };
}
