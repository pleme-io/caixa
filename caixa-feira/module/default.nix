# Home-manager module for the caixa ecosystem — drops `feira`, `caixa-lsp`,
# and the caixa.nvim plugin onto the user's PATH + nvim runtimepath.
#
# Usage (in your nix flake):
#
#   imports = [ caixa.homeManagerModules.default ];
#   programs.caixa = {
#     enable = true;
#     enableLsp = true;   # caixa-lsp on PATH
#     enableNvim = true;  # caixa.nvim treesitter + LSP wiring
#     theme = "dark";     # or "light"
#   };
#
# The feira binary is always installed when `enable = true`. LSP and nvim
# integration are opt-in so CI containers don't pull them in.

{ config, lib, pkgs, ... }:

let
  cfg = config.programs.caixa;
  caixaPkgs = config._module.args.caixaPackages or { };
in {
  options.programs.caixa = {
    enable = lib.mkEnableOption "the caixa tatara-lisp package system (feira CLI)";

    enableLsp = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Install caixa-lsp for tatara-lisp / caixa editor integration.";
    };

    enableNvim = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Install caixa.nvim — neovim plugin (treesitter + LSP wiring + blackmatter colors).";
    };

    theme = lib.mkOption {
      type = lib.types.enum [ "dark" "light" ];
      default = "dark";
      description = "Blackmatter palette variant for diagnostics + editor colors.";
    };

    defaultHost = lib.mkOption {
      type = lib.types.str;
      default = "github:pleme-io";
      description = ''
        Default Git host used when a dep omits `:fonte`. Written to
        `~/.config/caixa/config.yaml`. Override per-org as needed.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages =
      (if caixaPkgs ? feira then [ caixaPkgs.feira ] else [ ])
      ++ (lib.optional (cfg.enableLsp && caixaPkgs ? caixa-lsp) caixaPkgs.caixa-lsp);

    # XDG config — resolver defaults + theme variant.
    xdg.configFile."caixa/config.yaml".text = lib.generators.toYAML { } {
      default_host = cfg.defaultHost;
      theme = cfg.theme;
    };

    # Neovim plugin hook — surfaces caixa.nvim at the programs.neovim level
    # so a user's existing nvim setup picks it up.
    programs.neovim = lib.mkIf cfg.enableNvim {
      plugins = lib.optional (caixaPkgs ? caixa-nvim) caixaPkgs.caixa-nvim;
      extraLuaConfig = ''
        require("caixa").setup({
          theme = "${cfg.theme}",
          lsp_cmd = "${if caixaPkgs ? caixa-lsp then "${caixaPkgs.caixa-lsp}/bin/caixa-lsp" else "caixa-lsp"}",
          feira_cmd = "${if caixaPkgs ? feira then "${caixaPkgs.feira}/bin/feira" else "feira"}",
          format_on_save = true,
        })
      '';
    };
  };
}
