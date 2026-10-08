{ config, lib, pkgs, ... }:
let
  cfg = config.programs.morf.pattern;
  package = pkgs.callPackage ../pattern.nix {};
  rule = {
    order = 11600;
    control = "sufficient";
    modulePath = "${pkgs.pam}/lib/security/pam_exec.so";
    args = [ "quiet" "quiet_log" "expose_authtok" "/run/wrappers/bin/morf-pattern-check" "--check" ];
  };
in {
  options.programs.morf.pattern.enable = lib.mkOption {
    type = lib.types.bool;
    default = true;
    description = "Allow locally enrolled patterns for Morf lock and greetd. Enroll with sudo morf-pattern setup; credentials stay outside the Nix store.";
  };
  config = lib.mkIf (config.programs.morf.enable && cfg.enable) {
    environment.systemPackages = [ package ];
    security.wrappers.morf-pattern-check = {
      source = "${package}/libexec/morf-pattern-check";
      owner = "root";
      group = "root";
      setuid = true;
    };
    security.pam.services.morf-lock.rules.auth.morf-pattern = rule;
    security.pam.services.greetd.rules.auth.morf-pattern = rule;
  };
}
