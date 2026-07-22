{
  description = "caixa — the tatara-lisp package system + ecosystem";

  # substrate.rust.workspace dispatches over Cargo.gen.lock (the slim gen delta,
  # reconstructed to the full BuildSpec in pure Nix) — no crate2nix, no Cargo.nix.
  inputs = {
    substrate.url = "github:pleme-io/substrate";
    nixpkgs.follows = "substrate/nixpkgs";
  };

  outputs = { substrate, nixpkgs, ... }:
    let
      # The rust-workspace flake (apps/checks/devShells/overlays/packages).
      base = substrate.rust.workspace {
        src = ./.;
        member = "caixa-tatara";
      };

      # `feira` — the caixa CLI (crate `caixa-feira`, `[[bin]] name = "feira"`).
      # Grafted as a second per-system package the same way frost's flake
      # restores frost-complete-forge: one base workspace call stays
      # authoritative for apps/checks/devShells/overlays; the second member's
      # `default` package is added under its own binary name. Runtime-install
      # consumers (e.g. pleme-io/actions/caixa-render) reference this as
      # `github:pleme-io/caixa#feira` instead of `cargo install feira`.
      feiraBase = substrate.rust.workspace {
        src = ./.;
        member = "caixa-feira";
      };
      feiraSystems = [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" "aarch64-linux" ];
      withFeira = nixpkgs.lib.genAttrs feiraSystems (system:
        (base.packages.${system} or { }) // {
          feira = feiraBase.packages.${system}.default;
        });
    in
    base // {
      packages = withFeira;
    };
}
