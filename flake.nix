{
  description = "caixa — the tatara-lisp package system + ecosystem";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    # Substrate lends rust-service / rust-tool / rust-workspace builders. We
    # don't pull in its crate2nix path — the workspace goes through
    # `rustPlatform.buildRustPackage` directly so the Nix integration is
    # native (no compat layer).
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, substrate, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        # Single source-filter for every crate we build — excludes target/
        # and the operator's generated manifests so reruns are deterministic.
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let relPath = pkgs.lib.removePrefix (toString ./.) (toString path);
            in !(builtins.match "^/target(/.*)?$" relPath != null
                 || builtins.match "^/result.*$" relPath != null
                 || builtins.match ".*/\\.direnv(/.*)?$" relPath != null);
        };

        # Common builder — every binary gets the same vendored-Cargo path.
        mkRustBin = { pname, package, doCheck ? false }:
          pkgs.rustPlatform.buildRustPackage {
            inherit pname src doCheck;
            version = "0.1.0";
            cargoLock = {
              lockFile = ./Cargo.lock;
              # Git deps fall under outputHashes; tatara-lisp + iac-forge
              # come from sibling path deps so they never hit this map.
              outputHashes = { };
            };
            cargoBuildFlags = [ "-p" package ];
            cargoTestFlags = [ "-p" package ];
          };

        feira = mkRustBin { pname = "feira"; package = "caixa-feira"; };
        caixa-lsp = mkRustBin { pname = "caixa-lsp"; package = "caixa-lsp"; };
        caixa-operator = mkRustBin { pname = "caixa-operator"; package = "caixa-operator"; };

        caixa-nvim = pkgs.vimUtils.buildVimPlugin {
          pname = "caixa.nvim";
          version = "0.1.0";
          src = ./caixa.nvim;
          meta = {
            description = "Neovim integration for the caixa tatara-lisp ecosystem";
            license = pkgs.lib.licenses.mit;
          };
        };

        caixa-helm = pkgs.stdenvNoCC.mkDerivation {
          pname = "caixa-helm";
          version = "0.1.0";
          src = ./caixa-helm;
          dontBuild = true;
          installPhase = ''
            mkdir -p $out/share/caixa
            cp -r . $out/share/caixa/helm
          '';
        };

        # OCI image for the operator — consumed by caixa-flux HelmRelease.
        caixa-operator-image = pkgs.dockerTools.buildLayeredImage {
          name = "caixa-operator";
          tag = "0.1.0";
          contents = [ caixa-operator pkgs.cacert pkgs.gitMinimal ];
          config = {
            Entrypoint = [ "/bin/caixa-operator" ];
            User = "65532:65532";
            WorkingDir = "/var/cache/caixa";
            Env = [ "RUST_LOG=info,kube=warn,caixa_operator=info" ];
          };
        };
      in {
        packages = {
          inherit feira caixa-lsp caixa-operator caixa-nvim caixa-helm caixa-operator-image;
          default = feira;
        };

        apps.default = {
          type = "app";
          program = "${feira}/bin/feira";
        };

        devShells.default = pkgs.mkShell {
          name = "caixa-dev";
          packages = with pkgs; [
            rustc cargo rustfmt clippy rust-analyzer
            git gitMinimal openssh
            kubernetes-helm kubectl fluxcd
            tree-sitter
            jq yq-go
          ];
          shellHook = ''
            export CAIXA_DEV=1
            echo "caixa dev shell — feira / caixa-lsp / caixa-operator build via 'nix build .#<pname>'"
          '';
        };

        checks = {
          cargo-fmt = pkgs.runCommand "caixa-cargo-fmt" {
            inherit src;
            nativeBuildInputs = [ pkgs.rustfmt pkgs.cargo ];
          } ''
            cd $src
            cargo fmt --all --check
            touch $out
          '';

          # Full workspace test — picks up every crate's unit + integration tests.
          workspace-tests = pkgs.rustPlatform.buildRustPackage {
            pname = "caixa-workspace-tests";
            version = "0.1.0";
            inherit src;
            cargoLock.lockFile = ./Cargo.lock;
            cargoTestFlags = [ "--workspace" ];
            doCheck = true;
          };
        };

        # Forge-style substrate integration — the operator exposes the same
        # HM/NixOS module shape as every other pleme-io service.
        # Keep `legacyPackages` unused for `nix flake show` cleanliness.
      })
    // {
      homeManagerModules.default = import ./caixa-feira/module { };

      # Per-system NixOS module for running caixa-operator outside K8s
      # (e.g. on a dev laptop pointed at a remote kubeconfig).
      nixosModules.default = { config, lib, pkgs, ... }: with lib; let
        cfg = config.services.caixa-operator;
        pkg = self.packages.${pkgs.system}.caixa-operator;
      in {
        options.services.caixa-operator = {
          enable = mkEnableOption "caixa-operator (local kube client)";
          kubeconfig = mkOption {
            type = types.str;
            default = "/etc/kubernetes/admin.conf";
            description = "Path to the kubeconfig the operator watches.";
          };
          watchNamespace = mkOption {
            type = types.str;
            default = "";
            description = "Namespace to scope the watch; empty = cluster.";
          };
          logFormat = mkOption {
            type = types.enum [ "text" "json" ];
            default = "text";
          };
        };
        config = mkIf cfg.enable {
          systemd.services.caixa-operator = {
            description = "caixa CRD reconciler";
            wantedBy = [ "multi-user.target" ];
            after = [ "network.target" ];
            serviceConfig = {
              ExecStart = "${pkg}/bin/caixa-operator --log=${cfg.logFormat}${optionalString (cfg.watchNamespace != "") " --namespace=${cfg.watchNamespace}"}";
              Environment = [
                "KUBECONFIG=${cfg.kubeconfig}"
                "RUST_LOG=info,kube=warn"
              ];
              DynamicUser = true;
              NoNewPrivileges = true;
              ProtectSystem = "strict";
              ProtectHome = true;
              Restart = "on-failure";
              RestartSec = "5s";
            };
          };
        };
      };

      # Hook for substrate-consumer repos that want to import our builders.
      lib.substrate = substrate;
    };
}
