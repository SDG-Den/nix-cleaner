{
  description = "nix-cleaner: visual analysis of Nix store size, GC roots and closures";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {inherit system;};
        inherit (pkgs) lib;

        nativeBuildInputs = with pkgs; [
          pkg-config
          wrapGAppsHook4
        ];

        buildInputs = with pkgs; [
          gtk4
          libadwaita
          glib
          adwaita-icon-theme
          hicolor-icon-theme
          gsettings-desktop-schemas
        ];

        runtimePath = lib.makeBinPath (with pkgs; [
          nix
          coreutils
        ]);

        nix-cleaner = pkgs.rustPlatform.buildRustPackage {
          pname = "nix-cleaner";
          version = "0.1.0";
          src = lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = nativeBuildInputs ++ [pkgs.makeWrapper];
          inherit buildInputs;

          postInstall = ''
            install -Dm644 ${./nix-cleaner.desktop} \
              "$out/share/applications/nix-cleaner.desktop"

            # Only the gui is wrapped. The helper must stay a plain binary
            # because pkexec refuses to execute shell scripts.
            wrapProgram $out/bin/nix-cleaner \
              --prefix PATH : "${runtimePath}:/run/current-system/sw/bin"
          '';

          meta = with lib; {
            description = "Visual analysis of Nix store size, GC roots and closures";
            mainProgram = "nix-cleaner";
            license = licenses.gpl3Plus;
            platforms = platforms.linux;
          };
        };
      in {
        packages = {
          inherit nix-cleaner;
          default = nix-cleaner;
        };

        apps.default = {
          type = "app";
          program = "${nix-cleaner}/bin/nix-cleaner";
        };

        checks.default = nix-cleaner;

        devShells.default = pkgs.mkShell {
          nativeBuildInputs =
            nativeBuildInputs
            ++ (with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
              mold
            ]);

          buildInputs =
            buildInputs
            ++ (with pkgs; [
              nix
              nixd
              nixfmt
            ]);

          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
          RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
          CARGO_TARGET_DIR = "target";
          XDG_DATA_DIRS = lib.makeSearchPath "share" [
            pkgs.adwaita-icon-theme
            pkgs.hicolor-icon-theme
            pkgs.gsettings-desktop-schemas
            pkgs.gtk4
          ];
        };
      }
    )
    // {
      nixosModules.default = import ./modules/nixos.nix self;
      nixosModules.nix-cleaner = import ./modules/nixos.nix self;
    };
}
