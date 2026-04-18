//! `caixa-resolver` — git-only dependency resolver.
//!
//! **Store model, Zig-style.** A caixa is a Git repo whose root has a
//! `caixa.lisp`. There is no central registry: `feira` publishes by pushing
//! a Git tag, and other caixas consume it via `:fonte (:tipo git …)` or by
//! shorthand `(:nome "x")`, which the resolver expands to the configured
//! default host (`github:pleme-io/<nome>` by default).
//!
//! Resolution flow:
//!   1. Expand default sources using [`ResolverConfig`].
//!   2. For each dep, clone-or-fetch into the cache, check out the pinned
//!      `:rev` / `:tag` / `:branch`, read its `caixa.lisp`.
//!   3. BFS transitive deps; stop when every node is resolved.
//!   4. Build `LacreEntry` list with BLAKE3 `:conteudo` (git object id
//!      prefixed with `git:`) and `:fechamento` (hash of content + sorted
//!      transitive closure fechamentos).

extern crate self as caixa_resolver;

pub mod cache;
pub mod config;
pub mod git;
pub mod lisp_config;
pub mod resolve;
pub mod url;

pub use cache::CacheDir;
pub use config::ResolverConfig;
pub use lisp_config::ResolverConfigLisp;
pub use resolve::{ResolveError, resolve_lacre};
pub use url::expand_shorthand;
