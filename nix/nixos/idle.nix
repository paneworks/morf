{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.morf.idle;
in
{
  options.programs.morf.idle = {
    lockTimeout = lib.mkOption {
      type = lib.types.ints.unsigned;
      default = 300;
      description = "Idle seconds before locking; zero disables automatic locking.";
    };
    screenOffTimeout = lib.mkOption {
      type = lib.types.ints.unsigned;
      default = cfg.lockTimeout + 30;
      defaultText = lib.literalExpression "config.programs.morf.idle.lockTimeout + 30";
      description = "Idle seconds before turning off displays; zero disables idle display power-off.";
    };
    suspendTimeout = lib.mkOption {
      type = lib.types.ints.unsigned;
      default = 1800;
      description = "Idle seconds before suspending; zero leaves sleep to explicit actions such as closing the lid.";
    };
  };

  config = lib.mkIf (config.programs.morf.enable && config.programs.hyprland.enable) {
    services.hypridle.enable = true;
    system.build.hyprlandIdleDpms = pkgs.writeShellScript "hyprland-idle-dpms" ''
      action="''${1:-}"
      case "$action" in on|off) ;; *) exit 2 ;; esac
      # Hyprland accepts different dispatcher syntax for Lua and Hyprlang.
      if result=$(${config.programs.hyprland.package}/bin/hyprctl dispatch "hl.dsp.dpms({action = \"$action\"})" 2>&1) \
        && [ "$result" = ok ]; then
        exit 0
      fi
      exec ${config.programs.hyprland.package}/bin/hyprctl dispatch dpms "$action"
    '';
    # Pass an explicit system config: ~/.config/hypr remains an editable
    # dotfiles symlink, and switching compositors drops this entire block.
    environment.etc."xdg/hypr/hypridle.conf".text = ''
      general {
        lock_cmd = ${pkgs.systemd}/bin/systemctl --user start morf-idle-lock.service
        before_sleep_cmd = ${pkgs.systemd}/bin/loginctl lock-session
        after_sleep_cmd = ${config.system.build.hyprlandIdleDpms} on
        inhibit_sleep = 3
      }

      ${lib.optionalString (cfg.lockTimeout > 0) ''
      listener {
        timeout = ${toString cfg.lockTimeout}
        on-timeout = ${pkgs.systemd}/bin/loginctl lock-session
      }
      ''}

      ${lib.optionalString (cfg.screenOffTimeout > 0) ''
      listener {
        timeout = ${toString cfg.screenOffTimeout}
        on-timeout = ${config.system.build.hyprlandIdleDpms} off
        on-resume = ${config.system.build.hyprlandIdleDpms} on
      }
      ''}

      ${lib.optionalString (cfg.suspendTimeout > 0) ''
      listener {
        timeout = ${toString cfg.suspendTimeout}
        on-timeout = ${pkgs.systemd}/bin/systemctl suspend
      }
      ''}
    '';
    systemd.user.services.hypridle = {
      partOf = [ "graphical-session.target" ];
      serviceConfig.ExecStart = lib.mkForce [
        ""
        "${config.services.hypridle.package}/bin/hypridle --config /etc/xdg/hypr/hypridle.conf"
      ];
      restartTriggers = [ config.environment.etc."xdg/hypr/hypridle.conf".source ];
    };
    # Repeated idle/sleep requests reuse the same running lock process.
    systemd.user.services.morf-idle-lock = {
      description = "Morf lock screen requested by Hyprland idle management";
      partOf = [ "graphical-session.target" ];
      path = [ "/run/current-system/sw" ];
      serviceConfig = {
        Type = "exec";
        ExecStart = "/run/current-system/sw/bin/morf lock";
      };
    };
  };
}
