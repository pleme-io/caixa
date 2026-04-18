//! `caixa-lacre` — lock-file types and content + closure hashing for the
//! caixa tatara-lisp package system.
//!
//! A **lacre** (Portuguese for *wax seal*) is the frozen, deterministic
//! resolution of a caixa's dep graph. Every entry records the concrete
//! source, pinned version, content hash (the caixa as fetched) and closure
//! hash (content + all transitive closures, sorted). The root hash of a
//! lacre identifies the entire reproducible build — plug it into sui-cache
//! and you have a content-addressed binary cache key for the caixa closure.
//!
//! The `deflacre` form is itself a [`tatara_lisp::domain::TataraDomain`], so
//! reading a `lacre.lisp` is the same typed-parse path as `caixa.lisp`.

extern crate self as caixa_lacre;

pub mod entry;
pub mod hash;
pub mod lock;

pub use entry::LacreEntry;
pub use hash::{ContentHasher, closure_hash, hash_bytes, root_hash};
pub use lock::Lacre;
