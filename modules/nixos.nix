self: {
  config,
  pkgs,
  lib,
  ...
}: let
  cfg = config.programs.nix-cleaner;

  polkitAction =
    pkgs.writeTextDir "share/polkit-1/actions/dev.den.nix-cleaner.policy" ''
      <?xml version="1.0" encoding="UTF-8"?>
      <!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
      <policyconfig>
        <vendor>nix-cleaner</vendor>
        <action id="dev.den.nix-cleaner.clean">
          <description>Remove Nix garbage collection roots</description>
          <message>Authentication is required to remove the selected Nix GC roots</message>
          <defaults>
            <allow_any>no</allow_any>
            <allow_inactive>no</allow_inactive>
            <allow_active>auth_admin_keep</allow_active>
          </defaults>
          <annotate key="org.freedesktop.policykit.exec.path">${cfg.package}/bin/nix-cleaner-helper</annotate>
        </action>
      </policyconfig>
    '';
in {
  options.programs.nix-cleaner = {
    enable = lib.mkEnableOption "the nix-cleaner store analysis application and its privileged helper";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.nix-cleaner;
      description = "The nix-cleaner package to install.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [cfg.package polkitAction];
    security.polkit.enable = true;
  };
}
