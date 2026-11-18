{
  description = "OLX listing scraper with Discord notifications";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "olx-scraper";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            doCheck = true;
            meta = {
              description = "OLX listing scraper with Discord notifications";
              license = nixpkgs.lib.licenses.gpl3Plus;
              mainProgram = "olx-scraper";
            };
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              rust-analyzer
              rustc
              rustfmt
            ];
          };
        }
      );

      nixosModules.default =
        { config, lib, pkgs, ... }:
        let
          cfg = config.services.olx-scraper;
        in
        {
          options.services.olx-scraper = {
            enable = lib.mkEnableOption "OLX listing notifier";

            configFile = lib.mkOption {
              type = lib.types.path;
              description = ''
                Config file with Discord webhook/URL groups, separated by blank
                lines. The first line of a group is the webhook, the remaining
                lines are OLX search URLs.
              '';
              example = "/run/agenix/olx-scraper-config";
            };

            interval = lib.mkOption {
              type = lib.types.str;
              default = "5min";
              description = "How often to check for new listings (systemd time span)";
            };

            stateDir = lib.mkOption {
              type = lib.types.str;
              default = "/var/lib/olx-scraper";
              description = "Directory for the seen-listings state file";
            };

            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              description = "The olx-scraper package to use";
            };
          };

          config = lib.mkIf cfg.enable {
            systemd.services.olx-scraper = {
              description = "OLX listing scraper with Discord notifications";
              after = [ "network-online.target" ];
              wants = [ "network-online.target" ];

              serviceConfig = {
                Type = "oneshot";
                DynamicUser = true;
                StateDirectory = "olx-scraper";
                LoadCredential = [ "config:${cfg.configFile}" ];
                ExecStart = "${cfg.package}/bin/olx-scraper";
              };

              environment = {
                CONFIG_FILE = "%d/config";
                STATE_DIR = cfg.stateDir;
              };
            };

            systemd.timers.olx-scraper = {
              description = "Timer for OLX listing scraper";
              wantedBy = [ "timers.target" ];
              timerConfig = {
                OnBootSec = "5min";
                OnUnitActiveSec = cfg.interval;
                RandomizedDelaySec = "5min";
              };
            };
          };
        };
    };
}
