{
  config,
  lib,
  pkgs,
  ...
}:
let
  quietConfig = ''
    hl.config({
        misc = {
            disable_hyprland_logo = true,
            disable_splash_rendering = true,
            background_color = "rgb(000000)",
            disable_watchdog_warning = true,
            disable_xdg_env_checks = true,
        },
        ecosystem = { no_update_news = true, no_donation_nag = true },
    })
  '';
  hyprland = config.programs.hyprland.package;
  desktopConfig = pkgs.writeText "hyprland-session.lua" ''
    local apply_scale = dofile("${./scripts/hyprland-scale.lua}")(
      ${builtins.toJSON config.programs.morf.uiScaleFile}, ${toString config.programs.morf.uiScale})
    local root = (os.getenv("XDG_CONFIG_HOME") or (assert(os.getenv("HOME")) .. "/.config")) .. "/hypr"
    local file = io.open(root .. "/hyprland.lua", "r")
    if file then
      file:close()
      -- require registers the user's file with Hyprland's config watcher.
      package.path = root .. "/?.lua;" .. root .. "/?/init.lua;" .. package.path
      require("hyprland")
    else
      dofile("${hyprland}/share/hypr/hyprland.lua")
    end
    ${quietConfig}
    ${config.programs.morf.hyprland.extraConfig}
    apply_scale()
  '';
  desktop = pkgs.writeShellScriptBin "hyprland-session" ''
    exec ${pkgs.systemd}/bin/systemd-cat --identifier=hyprland -- \
      ${hyprland}/bin/start-hyprland -- --config /etc/xdg/hypr/session.lua "$@"
  '';
  sessions =
    pkgs.runCommand "hyprland-quiet-sessions"
      {
        passthru.providedSessions = hyprland.providedSessions;
      }
      ''
        mkdir -p "$out/share"
        cp -R ${hyprland}/share/wayland-sessions "$out/share/"
        chmod -R u+w "$out/share/wayland-sessions"
        substituteInPlace "$out/share/wayland-sessions/hyprland.desktop" \
          --replace-fail '${hyprland}/bin/start-hyprland' '${desktop}/bin/hyprland-session'
      '';
  cfg = config.programs.morf.greeter;
  session = pkgs.writeShellScript "morf-greeter-session" ''
    ${cfg.sessionSetup}
    ${config.system.build.morfLauncher}/bin/morf greet
    status=$?
    ${hyprland}/bin/hyprctl dispatch 'hl.dsp.exit()' >/dev/null 2>&1 \
      || ${hyprland}/bin/hyprctl dispatch exit >/dev/null 2>&1
    exit "$status"
  '';
  greeterConfig = pkgs.writeText "morf-greeter.lua" ''
    local apply_scale = dofile("${./scripts/hyprland-scale.lua}")(
      ${builtins.toJSON config.programs.morf.uiScaleFile}, ${toString config.programs.morf.uiScale})
    hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
    ${quietConfig}
    hl.config({
      animations = { enabled = false },
      general = { border_size = 0, gaps_in = 0, gaps_out = 0 },
      input = { kb_layout = "us" },
    })
    ${cfg.extraConfig}
    apply_scale()
    hl.on("hyprland.start", function()
      hl.exec_cmd("${session}")
    end)
  '';
in
{
  options.programs.morf.hyprland.extraConfig = lib.mkOption {
    type = lib.types.lines;
    default = "";
    description = "Profile-specific Lua applied after the user's Hyprland configuration.";
  };

  # Keep the standard session names and other installed desktop environments.
  # UWSM starts hyprland.desktop too, so both login paths use the same settings.
  options.services.displayManager.sessionPackages = lib.mkOption {
    apply =
      packages:
      if config.programs.morf.enable && config.programs.hyprland.enable then
        map (package: if package.outPath == hyprland.outPath then sessions else package) packages
      else
        packages;
  };

  options.programs.morf.greeter = {
    extraConfig = lib.mkOption {
      type = lib.types.lines;
      default = "";
      description = "Additional Lua for the dedicated Hyprland greeter.";
    };
    sessionSetup = lib.mkOption {
      type = lib.types.lines;
      default = "";
      description = "Shell setup before launching Morf in the greeter.";
    };
    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Environment variables for the greeter compositor and its children.";
    };
  };

  config = lib.mkMerge [
    (lib.mkIf (config.programs.morf.enable && config.programs.hyprland.enable) {
      environment.etc."xdg/hypr/session.lua".source = desktopConfig;
      environment.systemPackages = [
        desktop
        (lib.hiPrio sessions)
      ];
      system.build.hyprlandQuietSessions = sessions;
      system.build.hyprlandSessionConfig = desktopConfig;
    })
    (lib.mkIf config.programs.morf.enable {
      system.build.morfGreeterConfig = greeterConfig;
      system.build.morfGreeterCompositor = pkgs.writeShellScript "morf-greeter-compositor" ''
        ${lib.concatStringsSep "\n" (
          lib.mapAttrsToList (name: value: "export ${name}=${lib.escapeShellArg value}") cfg.environment
        )}
        # Capture even early startup messages, while retaining journal diagnostics.
        exec ${pkgs.systemd}/bin/systemd-cat --identifier=morf-greeter -- \
          ${hyprland}/bin/Hyprland --config ${greeterConfig}
      '';
    })
  ];
}
