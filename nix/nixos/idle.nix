{
  config,
  lib,
  pkgs,
  ...
}:
{
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

      listener {
        timeout = 300
        on-timeout = ${pkgs.systemd}/bin/loginctl lock-session
      }

      listener {
        timeout = 330
        on-timeout = ${config.system.build.hyprlandIdleDpms} off
        on-resume = ${config.system.build.hyprlandIdleDpms} on
      }

      listener {
        timeout = 1800
        on-timeout = ${pkgs.systemd}/bin/systemctl suspend
      }
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
