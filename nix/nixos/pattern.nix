{ config, lib, pkgs, ... }:
let
  cfg = config.programs.morf.pattern;
  package = pkgs.callPackage ../pattern.nix {};
  helper = "/run/wrappers/bin/morf-pattern-check";
  pamExec = "${pkgs.pam}/lib/security/pam_exec.so";
  rule = {
    order = 11600;
    control = "sufficient";
    modulePath = pamExec;
    args = [ "quiet" "quiet_log" "expose_authtok" helper "--check" ];
  };
  reset = {
    order = 20000;
    control = "optional";
    modulePath = pamExec;
    args = [ "quiet" "quiet_log" helper "--reset" ];
  };
in {
  options.programs.morf.pattern.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = ''
      Allow locally enrolled unlock patterns for Morf lock and greetd.
      A 4-9 dot pattern has far less entropy than a password and installs a
      setuid verifier that PAM accepts as sufficient, so enable it only where
      that trade-off is acceptable. Failed attempts earn growing cooldowns
      (30 s, 1 min, 5 min, 15 min); after 25 failures patterns stay disabled
      until a password login through greetd or `sudo morf-pattern reset`.
      Enroll with `sudo morf-pattern setup`; credentials stay outside the Nix store.
    '';
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
    security.pam.services.greetd.rules.account.morf-pattern-reset = reset;
  };
}
