//! `caixa-flake` — `flake.lisp`, the tatara-lisp expression of a Nix flake.
//!
//! Two modes:
//!   1. **Transpile (default now):** `flake.lisp` → `flake.nix` at build
//!      time. Full Nix-ecosystem interop.
//!   2. **Direct eval (future):** `sui` evaluates `flake.lisp` natively
//!      once sui's nixpkgs coverage catches up.
//!
//! Authoring surface:
//!
//! ```lisp
//! (defflake
//!   :descricao "pangea-tatara-aws"
//!   :entradas ((:nome "nixpkgs" :url "github:nixos/nixpkgs?ref=nixos-unstable")
//!              (:nome "substrate" :url "github:pleme-io/substrate"))
//!   :saidas (:pacotes ((:nome "default" :src ".")
//!                      (:nome "teste" :src "./spec"))))
//! ```

extern crate self as caixa_flake;

pub mod flake;
pub mod render;

pub use flake::{FlakeInput, FlakeLisp, FlakeOutput, FlakePackage};
pub use render::render_flake_nix;
