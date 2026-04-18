//! `caixa-teia` — Lisp-native abstract synthesizer.
//!
//! The tatara-lisp sibling of Pangea's Ruby `AbstractSynthesizer`. Users
//! author `(defteia …)` forms to declare concrete resource instances; the
//! synthesizer collects them into a [`TeiaManifest`] that downstream
//! backends (HCL, JSON, Pangea Ruby, Lisp, ferrite Go) render directly.
//!
//! **Authoring surface:**
//!
//! ```lisp
//! (defteia
//!   :tipo        aws/vpc
//!   :nome        main
//!   :atributos   (:cidr-block "10.0.0.0/16"
//!                 :tags       (:name "main")))
//!
//! (defteia
//!   :tipo        aws/internet-gateway
//!   :nome        main
//!   :atributos   (:vpc-id (ref aws/vpc main id)))
//!
//! (defarquitetura secure-vpc
//!   :parametros (:profile Dev)
//!   :realizacao (list (referenciar aws/vpc    main)
//!                     (referenciar aws/igw    main)))
//! ```
//!
//! Phase-2 scope:
//!   - [`TeiaInstance`] — one resource instance (type + name + attributes)
//!   - [`TeiaManifest`] — collection of instances from one source
//!   - [`TeiaValue`] — recursive attribute value: scalar | list | object | ref
//!   - Parsed from tatara-lisp via the `defteia` keyword derive
//!   - `instance.to_hcl_json()` — minimal inline renderer (full backends in
//!     caixa-teia-forge)
//!
//! Integration with `iac-forge::IacResource` is optional: supply a schema
//! and [`TeiaInstance::validate`] checks required/readable attribute names
//! match the IR. Without a schema, instances are emitted as-is (fast path
//! for the common case where the user hand-rolls resources).

extern crate self as caixa_teia;

pub mod instance;
pub mod manifest;
pub mod reference;
pub mod value;

pub use instance::TeiaInstance;
pub use manifest::{TeiaManifest, parse_teia_source};
pub use reference::TeiaRef;
pub use value::TeiaValue;
