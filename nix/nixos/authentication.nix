{ config, lib, pkgs, ... }:
let
  sudo = config.security.pam.services.sudo.rules;
  authStep = pkgs.writeShellScript "morf-auth-step" (builtins.replaceStrings
    [ "PATH=/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" ]
    [ "PATH=${lib.makeBinPath [ pkgs.coreutils pkgs.util-linux ]}" ]
    (builtins.readFile ../../tools/pam/morf-auth-step));
  marker = order: step: {
    inherit order;
    control = "optional";
    modulePath = "${config.security.pam.package}/lib/security/pam_exec.so";
    args = [ "quiet" "${authStep}" step ];
  };
in
{
  config = lib.mkIf config.programs.morf.enable {
    security.polkit.enable = true;
    services.gnome.gnome-keyring.enable = true;

    security.pam.services = {
      greetd = {
        enableGnomeKeyring = true;
        fprintAuth = false;
      };
      morf-lock = {
        enableGnomeKeyring = true;
        fprintAuth = false;
      };
      morf-lock-finger = lib.mkIf config.services.fprintd.enable {
        text = ''
          auth sufficient ${config.services.fprintd.package}/lib/security/pam_fprintd.so
          auth required ${config.security.pam.package}/lib/security/pam_deny.so
          account include morf-lock
        '';
      };
      # The validators remain NixOS's PAM rules. These optional markers only
      # report progress to the shell; they never receive the password.
      sudo.rules = lib.mkIf config.security.sudo.enable {
        auth.morf-finger = (marker (sudo.auth.fprintd.order - 1) "finger") // {
          enable = sudo.auth.fprintd.enable;
        };
        auth.morf-password = marker
          ((if sudo.auth.unix-early.enable or false
            then sudo.auth.unix-early.order else sudo.auth.unix.order) - 1)
          "password";
        account.morf-ok = marker (sudo.account.unix.order + 1) "ok";
      };
    };
  };
}
