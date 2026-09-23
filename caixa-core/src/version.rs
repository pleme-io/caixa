use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A caixa's pinned version — a thin typed wrapper over a String that parses
/// as [`semver::Version`] on demand.
///
/// Stored as a String at rest so authoring a `caixa.lisp` stays a single
/// quoted literal. The typed form is reached through [`Self::parse`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct CaixaVersion(pub String);

impl CaixaVersion {
    /// Parse and validate the wrapped string as semver.
    pub fn parse(&self) -> Result<semver::Version, VersionError> {
        semver::Version::parse(&self.0)
            .map_err(|e| VersionError::semver(self.0.clone(), e.to_string()))
    }

    /// Borrow the string form.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for CaixaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for CaixaVersion {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CaixaVersion {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Substrate-canonical stdlib [`std::str::FromStr`] parse-set entry point on
/// the [`CaixaVersion`] newtype primitive — closes the canonical
/// `str::parse::<CaixaVersion>()` axis on the paired [`From<&str> for
/// CaixaVersion`] / [`From<String> for CaixaVersion`] infallible
/// forward-projection constructors. Delegates byte-for-byte through the
/// paired borrowed-input `impl From<&str> for CaixaVersion` immediately
/// above (which wraps `s.to_string()` into the newtype's inner `String`
/// slot), so every consumer that reaches [`CaixaVersion`] through the
/// stdlib `T: FromStr`-bounded parse surface (`str::parse::<CaixaVersion>`,
/// a `clap::Parser`-derived `#[arg(value_parser)]` on a future `feira
/// publish --versao <ver>` arg-parse, a `serde_with::DisplayFromStr`
/// wrapper on the [`crate::Caixa::versao`] field in a downstream typed-YAML
/// derive, or any generic `fn parse_versao<T: FromStr>(s: &str) -> Result<T,
/// T::Err>` receiver) routes through the same [`String::to_string`]-shaped
/// wrap the paired `From<&str>` constructor already exercises. `type Err =
/// std::convert::Infallible` because the paired `From<&str>` constructor is
/// total — [`CaixaVersion`] stores the wrapped string raw at rest (the
/// author surface's single-quoted `:versao "…"` literal round-trips
/// byte-for-byte) and defers semver validation to the paired
/// [`CaixaVersion::parse`] `Result<semver::Version, VersionError>`
/// accessor, so no byte-string the standard-library parse-set entry point
/// receives can fail construction on this axis (any `&str` is a valid
/// `CaixaVersion` body at rest; only `.parse::<semver::Version>()` on the
/// wrapped body can reject a shape the semver grammar refuses). Peer of
/// the paired `impl FromStr for String` stdlib impl on the standard-library
/// `String` newtype (whose `Err = Infallible` covers the same "any `&str` is
/// a valid `String` body" total-wrap discipline the [`CaixaVersion`]
/// newtype installs on the caixa-core surface). The first standard-library
/// stdlib-parse-set entry point on the [`CaixaVersion`] newtype beyond the
/// paired forward-projection [`From<&str>`] / [`From<String>`]
/// constructors and the sibling [`fmt::Display`] / [`AsRef<str>`] /
/// [`std::borrow::Borrow<str>`] projections the newtype already carries.
///
/// # Compounding
///
/// The stdlib parse-set entry point is the canonical Rust-idiomatic axis
/// generic bounds compose against: `str::parse::<T>()` is a `T: FromStr`-
/// bounded generic (not a `T: for<'a> TryFrom<&'a str>`-bounded one), so
/// lifting the axis onto [`CaixaVersion`] unlocks the `.parse::<CaixaVersion>()`
/// short-form on every future stdlib-shaped consumer without forcing the
/// caller to spell the paired `From<&str>` constructor at the wire-up site.
/// A future `clap::Args`-derived `feira publish --versao <ver>` arg-parse
/// composes `arg.parse::<CaixaVersion>()` directly through the
/// `#[arg(value_parser = clap::value_parser!(CaixaVersion))]` short-form
/// (which resolves through the `T: FromStr` bound `clap::value_parser!`
/// installs on any type carrying the trait), a `serde_with::DisplayFromStr`
/// wrapper on a future typed-YAML [`crate::Caixa::versao`] field routes
/// through the same `T: FromStr` bound `serde_with` keys off, and any
/// generic per-authored-string coalescer over a mixed newtype family
/// (`Result<T, T::Err>` on a `T: FromStr` bound) picks up
/// [`CaixaVersion`] as one of its arms by construction.
///
/// # Round-trip discipline
///
/// The `Err = Infallible` shape witnesses the round-trip discipline the
/// paired forward-projection [`fmt::Display`] impl closes at compile time:
/// `s.parse::<CaixaVersion>().unwrap().to_string() == s` for every `&str`
/// (the fail-before-pass-after pin
/// [`caixa_version_from_str_round_trips_through_display_on_every_input`]
/// witnesses this against the sibling `caret_matches_minor_range` /
/// `star_is_any` / `caixa_version_as_str_accessor_is_const_fn` fixture
/// bodies covering the semver-shape, prerelease-shape, empty-body,
/// requirement-shape, and non-semver-junk corners). Any future accidental
/// narrowing (a stray `parse_semver_first` validation gate slipping onto
/// the wrap path, a normalization step that would drop whitespace or
/// canonicalize a prerelease tag) trips the pin at caixa-core build time
/// under the byte-equality assertion, refusing the divergent shape ahead
/// of the downstream materializer's admit cycle.
impl std::str::FromStr for CaixaVersion {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Delegate byte-for-byte through the paired borrowed-input
        // `impl From<&str> for CaixaVersion` constructor above — the
        // total-wrap axis every stdlib `T: FromStr`-bounded consumer
        // reaches [`CaixaVersion`] through resolves to the same
        // [`String::to_string`]-shaped body the sibling forward-
        // projection constructor already installs. `type Err =
        // Infallible` because the paired constructor is total; `Ok`
        // is the only reachable arm on this axis.
        Ok(<Self as From<&str>>::from(s))
    }
}

/// Substrate-canonical [`AsRef<str>`] projection on the [`CaixaVersion`]
/// typed newtype — routes through the same [`CaixaVersion::as_str`]
/// `pub const fn` scalar accessor the sibling [`fmt::Display`] impl
/// and every downstream `&str`-shaped consumer already keys off, so
/// any future consumer that binds a [`CaixaVersion`] through the
/// standard-library `impl AsRef<str>` bound (a `Path`-shaped file-
/// system reader on the operator side that accepts the version body
/// as one segment of a per-caixa `versao/<v>/...` on-disk cache path,
/// a builder-shaped API on the future `feira publish` writer verb
/// that composes `<prefix><versao>` through a git-tag builder crate's
/// `impl AsRef<str>` join step, a `HashMap<CaixaVersion, _>` lookup
/// through the `map.get::<str>(v.as_ref())` shape a future
/// version-keyed dispatch table lands on) reaches the wrapped
/// [`String`] through one substrate-primitive dispatch rather than
/// through the pre-lift `.as_str()` open-coded projection at every
/// wire-up.
///
/// Peer of the sibling [`fmt::Display`] impl on the same primitive —
/// both delegate to the shared [`CaixaVersion::as_str`] `pub const
/// fn` accessor, so [`format!("{v}")`], `v.as_str()`, and
/// `<CaixaVersion as AsRef<str>>::as_ref(&v)` resolve to the same
/// byte-string per instance by construction. A future rebrand of the
/// wrapped storage (a hypothetical widening to a typed [`semver::Version`]
/// slot the roadmap acknowledges once eager parse-on-construct
/// discipline lands, an internal normalization step that trims
/// leading zeroes off pre-release identifiers, a per-cluster overlay
/// the operator pins through a future `:versao-overrides` slot) that
/// changes what [`CaixaVersion::as_str`] returns migrates every
/// consumer of every one of the three paths in lockstep.
///
/// Same "route the trait impl through the substrate-primitive
/// accessor" discipline the sibling [`fmt::Display`] impl on this
/// type already carries — extends it onto the standard-library
/// [`AsRef<str>`] projection axis every third-party API that takes
/// `impl AsRef<str>` (the [`std::path::Path::new`] / [`std::fs`]
/// interop surface, [`std::process::Command::arg`], the peer
/// `tracing::field::Value` recorder's `Str`-arm, every `clap`-side
/// `value_parser!` fold that accepts an owned newtype through
/// `impl AsRef<str>`) already binds through. Rust-side newtype
/// convention pairs `AsRef<str>` and [`fmt::Display`] on the same
/// primitive so a caller who has one has both; before this lift,
/// [`CaixaVersion`] carried [`fmt::Display`] but not the paired
/// [`AsRef<str>`] impl the convention names.
///
/// The first standard-library trait added to [`CaixaVersion`] beyond
/// the pre-existing [`serde::Serialize`] / [`serde::Deserialize`] /
/// [`Debug`] / [`Clone`] / [`PartialEq`] / [`Eq`] / [`Hash`] derives
/// and the paired [`fmt::Display`] / [`From<String>`] / [`From<&str>`]
/// hand-written impls. Pinned load-bearing by
/// [`tests::caixa_version_as_ref_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`]) — any future
/// silent detour that routes the impl through a divergent projection
/// (a `Cow<'_, str>` intermediate, a stray `.to_lowercase()`
/// normalization, a swap onto a per-arm inline `&self.0.as_str()`
/// re-inlining) trips at caixa-core test time under `assert_eq!`
/// rather than at a downstream `impl AsRef<str>`-bound consumer's
/// silent split.
impl AsRef<str> for CaixaVersion {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Trait-idiomatic *HashMap-key-shaped* borrow projection on the
/// [`CaixaVersion`] newtype primitive — the standard-library
/// [`std::borrow::Borrow<str>`] companion to the paired sibling
/// [`AsRef<str>`] impl (a086 lift) on the same borrow-projection axis of
/// this primitive. Routes byte-for-byte through the substrate-primitive
/// [`CaixaVersion::as_str`] `pub const fn` accessor — the same accessor
/// the paired [`AsRef<str>`] and [`fmt::Display`] impls already delegate
/// through — so every consumer that binds a [`CaixaVersion`] through the
/// standard-library `Borrow<str>` bound reaches the wrapped byte-string
/// through one substrate-primitive dispatch rather than through a
/// pre-lift `.as_str()` open-coded projection at every wire-up.
///
/// A future consumer that wants to key a map or set by
/// [`CaixaVersion`] and look up entries by a borrowed [`&str`] — a
/// per-`:versao` compatibility matrix `HashMap<CaixaVersion, PolicyRow>`
/// where the reconciliation loop's per-cycle `.get(current_versao_str)`
/// probes the map with the raw `&str` view of the current cluster
/// snapshot's version body (the `HashMap::get<Q: ?Sized>` signature is
/// `where K: Borrow<Q>, Q: Hash + Eq`; without this impl the caller must
/// wrap the borrowed `&str` in a fresh [`CaixaVersion`] allocation on
/// every probe), a future `BTreeMap<CaixaVersion, _>::range(..)` sweep
/// over a per-versao index that accepts a borrowed `&str` range bound
/// through the same `Borrow<str>` bound, a
/// `HashSet<CaixaVersion>::contains(&str)` membership probe on a
/// per-versao denylist keyed by owned [`CaixaVersion`] but queried by
/// the borrowed view — reaches the wrapped byte-string through this one
/// dispatch on the substrate primitive, without the pre-lift
/// `CaixaVersion::from(<&str>)` per-probe allocation the paired forward
/// [`From<&str> for CaixaVersion`] constructor would otherwise force at
/// every lookup site.
///
/// Peer of the sibling [`AsRef<str>`] impl on the same borrow-projection
/// axis — both project a borrowed `&self` binding onto a borrowed `&str`
/// via the shared substrate-primitive [`CaixaVersion::as_str`] accessor.
/// Rust's standard library deliberately splits the two trait axes on the
/// two bounds they carry: [`AsRef<str>`] is the *conversion* bound used
/// by APIs that accept `impl AsRef<str>` and view the input as a `&str`
/// projection (the [`std::path::Path::new`] / [`std::fs`] interop
/// surface, [`std::process::Command::arg`], [`clap`]-side
/// `value_parser!` folds), while [`std::borrow::Borrow<str>`] is the
/// stricter *identity* bound the collection APIs
/// ([`std::collections::HashMap`], [`std::collections::BTreeMap`],
/// [`std::collections::HashSet`], [`std::collections::BTreeSet`]) key
/// their lookup surfaces off — [`std::borrow::Borrow`] additionally
/// promises that a borrowed view produced through [`Borrow::borrow`]
/// hashes and compares byte-identically to the owned form, which is the
/// contract [`std::collections::HashMap::get`] relies on when it hashes
/// the query key through `Q` (`str`) and matches against slot keys
/// hashed through `K` ([`CaixaVersion`]). The [`CaixaVersion`] newtype
/// meets that contract by construction: the derived [`Hash`] impl hashes
/// the wrapped [`String`] field, which (through the standard-library
/// `impl Hash for String { fn hash(...) { (**self).hash(...) } }`
/// pass-through) dispatches to [`str::hash`] on the raw bytes — the same
/// dispatch a direct `.hash()` on the `&str` returned by
/// [`Self::borrow`] would take. The derived [`PartialEq`] and [`Eq`]
/// impls compare field-wise (byte-equal on the wrapped [`String`]), so
/// `cv1 == cv2` reduces to `cv1.borrow() == cv2.borrow()` at the
/// `&str`-projection axis. Both invariants — hash-agrees and
/// eq-agrees — hold structurally, so this impl is sound under the
/// [`std::borrow::Borrow`] documented safety contract.
///
/// Same "one substrate-primitive dispatch, one shared accessor" discipline
/// the paired [`AsRef<str>`] impl on this primitive already carries —
/// extends it onto the [`std::borrow::Borrow<str>`] projection axis the
/// standard-library collection APIs key their `.get::<Q>` /
/// `.contains::<Q>` / `.range::<R, T>` / `.remove::<Q>` lookup surfaces
/// off. Rust's standard library mirrors this exact pairing on its own
/// [`String`] primitive (`impl AsRef<str> for String` +
/// `impl Borrow<str> for String`), so a newtype that carries one axis
/// but not the other splits off the convention that lets every
/// [`String`]-shaped consumer swap the newtype in without re-shaping
/// its bounds.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_borrow_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`] on the same
/// instance),
/// [`tests::caixa_version_borrow_str_and_as_ref_str_agree_on_every_shape`]
/// (cross-axis partition pin against the paired [`AsRef<str>`] impl,
/// closing the "borrow-axis two-corner split" bifurcation on the same
/// wrapped body), and
/// [`tests::caixa_version_borrow_str_enables_hashmap_lookup_by_borrowed_key`]
/// (contract-witness pin routing a [`std::collections::HashMap::get`]
/// probe against a `&str` key through the `Borrow<str>` bound on a map
/// keyed by owned [`CaixaVersion`], asserting the collection APIs reach
/// the same slot the borrowed and owned forms compose the same hash for).
impl std::borrow::Borrow<str> for CaixaVersion {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

/// Trait-idiomatic *owned-`String`* reverse projection on the
/// [`CaixaVersion`] newtype primitive — the owned-heap-string inverse
/// of the pre-existing [`From<String> for CaixaVersion`] /
/// [`From<&str> for CaixaVersion`] forward-projection pair on this
/// primitive. Returns the wrapped [`String`] verbatim ([`Self::0`],
/// a move of the pre-existing heap allocation — no re-copy of the
/// per-instance version body's bytes), so every consumer that binds a
/// [`CaixaVersion`] through the standard-library `.into()` /
/// [`From<Self> for String`] (equivalently [`Into<String>`]) axis
/// reaches the wrapped byte-string through one substrate-primitive
/// dispatch rather than through a `.as_str().to_owned()` /
/// `.to_string()` allocating detour whose bounds have no compile-time
/// link back to the newtype's storage.
///
/// A future consumer that wants to unwrap a [`CaixaVersion`] into an
/// owned [`String`] — a `serde_json::Value::String(versao.into())`
/// structured-payload composer where the `Value::String` arm typing
/// demands an owned [`String`] and the sibling
/// [`AsRef<str>`]-borrowed axis forces an explicit `.to_owned()`
/// restatement at every call site, a future
/// `HashMap::<String, _>::from_iter([(versao.into(), _)])` per-versao
/// lookup where the map's key type is owned [`String`] rather than
/// [`&str`] borrowed from a stashed [`CaixaVersion`], a future
/// `Cow::<'static, str>::Owned(versao.into())` composer where the
/// owned arm typing rules out the borrowed [`AsRef<str>`] return —
/// reaches the wrapped [`String`] through this one dispatch, avoiding
/// the pre-lift double-allocation (`.as_str().to_owned()` on the owned
/// path would allocate a fresh [`String`] rather than reuse the
/// wrapper's own heap allocation).
///
/// Opens the trait-idiomatic *owned-`String`* reverse-projection axis
/// on the substrate's core String-wrapper newtype primitive
/// [`CaixaVersion`], mirroring the paired owned-`String` forward-
/// projection family the sibling closed-set fieldless typed enums
/// ([`crate::supervisor::RestartStrategy`] (7baa18a, first-mover),
/// [`crate::supervisor::RestartPolicy`] (7851725),
/// [`crate::CaixaKind`] (per its own doc block, third peer), plus the
/// remaining twelve closed-set enums) already carry — Rust's standard
/// library does not derive `From<Self> for String` from `From<String>
/// for Self`, so every newtype that carries a forward `From<String>`
/// constructor but not the paired reverse-unwrap axis forces every
/// call site through a `.to_string()` / `.as_str().to_owned()` detour
/// that allocates fresh bytes rather than moving the wrapper's own
/// heap allocation.
///
/// Preserves the two-path split on the wrapped byte-string: the paired
/// [`AsRef<str>`] and [`fmt::Display`] impls stay reachable for the
/// borrowed `&str` and formatter-output paths, this impl closes the
/// owned-`String` reverse axis. Same "one dispatch on the substrate
/// primitive" discipline the peer forward `From<String> for
/// CaixaVersion` / `From<&str> for CaixaVersion` constructors carry,
/// now extended onto the owned-heap-string reverse projection.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_into_owned_string_returns_wrapped_body`]
/// (byte-parity pin against [`CaixaVersion::as_str`] on the same
/// instance) and
/// [`tests::caixa_version_from_into_owned_string_and_as_str_agree_on_every_shape`]
/// (cross-axis partition pin against the paired borrowed
/// [`AsRef<str>`] impl and the sibling [`fmt::Display`]-routed
/// [`ToString::to_string`] surface, plus a round-trip witness through
/// the paired forward [`From<String> for CaixaVersion`] constructor
/// closing the two-way `Self → String → Self` cycle by construction).
impl From<CaixaVersion> for String {
    fn from(v: CaixaVersion) -> String {
        v.0
    }
}

/// Trait-idiomatic *borrowed-input, owned-`String` output* reverse
/// projection on the [`CaixaVersion`] newtype primitive — the
/// borrowed-input companion to the paired owned-input
/// [`From<CaixaVersion> for String`] impl immediately above. Routes
/// byte-for-byte through the substrate-primitive
/// [`CaixaVersion::as_str`] `pub const fn` accessor (via
/// [`str::to_owned`]) so every consumer that holds a
/// borrowed [`&CaixaVersion`] and needs an owned [`String`] — a
/// `[…].iter().map(String::from).collect::<Vec<_>>()` per-instance
/// materializer over `&[CaixaVersion]` (whose iterator yields
/// `&CaixaVersion`, not `CaixaVersion`, so the owned-input
/// [`From<CaixaVersion> for String`] axis alone forces every call site
/// through an explicit `.clone()` / dereference restatement), a future
/// `HashMap::<String, _>::from_iter` that keys off a borrowed-
/// iteration axis where cloning the wrapper would allocate one
/// [`String`] beyond the map entry's own, a future
/// `serde_json::Value::String(String::from(&caixa.versao))`
/// structured-payload composer that owns the emit-path without moving
/// out of a borrowed field — reaches the wrapped byte-string through
/// this one dispatch on the substrate primitive.
///
/// Second corner on the `{Self, &Self} → String` reverse-projection
/// family opened on the paired owned-input
/// [`From<CaixaVersion> for String`] impl immediately above. Rust's
/// `From` trait does not derive the `From<&Self>` sibling from a
/// `From<Self>` impl (the blanket
/// `impl<T, U> From<&T> for U where T: Clone, U: From<T>` does not
/// exist in `core`), so every newtype that carries the owned-input
/// reverse axis but not the borrowed-input axis forces every borrowed
/// call site through a `.clone()` / `<String>::from(v.clone())` detour
/// whose type bounds have no compile-time link back to the newtype.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_borrowed_into_owned_string_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`] via a borrowed
/// input) and
/// [`tests::caixa_version_from_owned_and_borrowed_into_string_agree_on_every_shape`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaVersion> for String`] impl on the same instance,
/// closing the "owned-input move vs. borrowed-input clone" bifurcation
/// on the same wrapped body).
impl From<&CaixaVersion> for String {
    fn from(v: &CaixaVersion) -> String {
        v.as_str().to_owned()
    }
}

/// Trait-idiomatic *owned-input, [`std::borrow::Cow<'static, str>`]
/// output* reverse projection on the [`CaixaVersion`] newtype
/// primitive — the [`Cow<'static, str>`] companion to the paired
/// owned-input [`From<CaixaVersion> for String`] impl (999a310) on
/// the same primitive. Routes through
/// [`std::borrow::Cow::Owned`]`(v.0)`, moving the wrapper's own heap
/// allocation through verbatim (no re-copy of the per-instance version
/// body's bytes, no allocating detour through
/// [`CaixaVersion::as_str`] + [`str::to_owned`]) — so every consumer
/// that binds a [`CaixaVersion`] through the standard-library `.into()`
/// / [`From<Self> for Cow<'static, str>`] axis reaches the wrapped
/// byte-string through one substrate-primitive dispatch on the exact
/// same heap allocation the manifest-parse forward
/// [`From<String> for CaixaVersion`] constructor accepted.
///
/// A future consumer that wants a [`Cow<'static, str>`]-typed handle
/// on a [`CaixaVersion`] — a
/// `metric_label: Cow<'static, str> = versao.into()` structured-log
/// key on a future per-caixa `caixa-operator` reconciliation counter
/// (whose emit surface types metric keys as `Cow<'static, str>` so
/// static compile-time literals and dynamic version bodies share the
/// same key-slot without an unconditional heap allocation on the
/// literal path), a future
/// `HashMap::<Cow<'static, str>, _>::from_iter([(versao.into(), _)])`
/// per-versao lookup where the map's key type is
/// [`Cow<'static, str>`] rather than owned [`String`] so
/// literal-lifetime keys can share the same map without wrapping in an
/// extra [`String`] allocation, a future M4 admission-webhook
/// rejection body whose per-arm error message composes through
/// `format!("{}", Cow::<'static, str>::from(caixa.versao))` where the
/// [`Cow<'static, str>`] intermediate is what the sibling error-frame
/// composer accepts — reaches the wrapped byte-string through this
/// one dispatch, without the pre-lift `.to_string().into()` /
/// `Cow::Owned(String::from(v))` double-hop that would allocate a
/// fresh intermediary [`String`] on the way to the same
/// [`Cow::Owned`] arm.
///
/// Deliberately returns [`std::borrow::Cow::Owned`] rather than
/// [`std::borrow::Cow::Borrowed`] — the substrate-primitive
/// [`CaixaVersion::as_str`] accessor's return does not carry the
/// `&'static str` lifetime by construction (the wrapped [`String`]
/// storage is a runtime heap allocation, not a compile-time literal),
/// so the [`Cow<'static, str>`] output shape rules out the borrowed
/// arm and the owned arm is the type-correct projection. Peer of the
/// paired owned-input [`From<CaixaVersion> for String`] impl on the
/// same primitive — both route through the wrapper's own heap
/// allocation via a move on `v.0`, preserving the zero-copy
/// discipline the substrate opens on its String-wrapper newtype
/// primitive.
///
/// Opens the trait-idiomatic *owned-input, [`Cow<'static, str>`]*
/// reverse-projection axis on the substrate's core String-wrapper
/// newtype primitive [`CaixaVersion`], mirroring the paired
/// [`Cow<'static, str>`] *forward*-projection family the sibling
/// closed-set fieldless typed enums (via
/// [`crate::supervisor::RestartStrategy`],
/// [`crate::supervisor::RestartPolicy`], and the remaining twelve
/// closed-set enums) already carry — on the enum peers, the paired
/// axis returns [`Cow::Borrowed`] because the accessor returns
/// `&'static str`; on this newtype the paired axis returns
/// [`Cow::Owned`] because the wrapped storage is runtime-allocated.
/// Rust's standard library does not derive `From<Self> for
/// Cow<'static, str>` from `From<Self> for String` (nor derive
/// `From<&Self>` from `From<Self>`), so every newtype that carries a
/// reverse `From<Self> for String` unwrap axis but not the paired
/// [`Cow<'static, str>`] axis forces every
/// [`Cow<'static, str>`]-typed call site through a `.to_string().into()`
/// double-allocation detour that heap-allocates a fresh intermediary
/// [`String`] between the wrapper and the [`Cow::Owned`] arm.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_into_owned_cow_str_returns_owned_wrapped_body`]
/// (byte-parity + [`Cow::Owned`]-arm pin against
/// [`CaixaVersion::as_str`] on the same instance, plus a round-trip
/// witness through the paired [`From<String> for CaixaVersion`]
/// constructor) and
/// [`tests::caixa_version_from_into_owned_cow_str_and_string_agree_on_every_shape`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaVersion> for String`] impl on the same instance,
/// closing the "owned-input into [`String`] vs. owned-input into
/// [`Cow<'static, str>`]" bifurcation on the same wrapped body).
impl From<CaixaVersion> for std::borrow::Cow<'static, str> {
    fn from(v: CaixaVersion) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Owned(v.0)
    }
}

/// Trait-idiomatic *borrowed-input, [`std::borrow::Cow<'static, str>`]
/// output* reverse projection on the [`CaixaVersion`] newtype
/// primitive — the borrowed-input companion to the paired owned-input
/// [`From<CaixaVersion> for std::borrow::Cow<'static, str>`] impl
/// immediately above. Routes byte-for-byte through the
/// substrate-primitive [`CaixaVersion::as_str`] `pub const fn`
/// accessor (via [`str::to_owned`] wrapped in
/// [`std::borrow::Cow::Owned`]) so every consumer that holds a
/// borrowed [`&CaixaVersion`] and needs a [`Cow<'static, str>`] —
/// a `[…].iter().map(Cow::<'static, str>::from).collect::<Vec<_>>()`
/// per-instance materializer over `&[CaixaVersion]` (whose iterator
/// yields `&CaixaVersion`, not `CaixaVersion`, so the paired
/// owned-input [`From<CaixaVersion> for Cow<'static, str>`] axis
/// alone forces every call site through an explicit `.clone()` /
/// dereference restatement), a future
/// `HashMap::<Cow<'static, str>, _>::from_iter` that keys off a
/// borrowed-iteration axis where cloning the wrapper would allocate
/// one [`String`] beyond the eventual [`Cow::Owned`] arm's own, a
/// future generic
/// `<T: for<'a> Into<Cow<'static, str>>>`-bound emitter on a
/// per-caixa diagnostic column that walks the
/// `iter().map(Into::into)` shape verbatim — reaches the wrapped
/// byte-string through this one dispatch on the substrate primitive.
///
/// Deliberately returns [`std::borrow::Cow::Owned`] rather than
/// [`std::borrow::Cow::Borrowed`] — the substrate-primitive
/// [`CaixaVersion::as_str`] accessor's return does not carry the
/// `&'static str` lifetime by construction, so the
/// [`Cow<'static, str>`] output shape rules out the borrowed arm and
/// the owned arm is the type-correct projection (mirroring the paired
/// owned-input impl's own [`Cow::Owned`] discipline). Second corner
/// on the `{Self, &Self} → Cow<'static, str>` reverse-projection
/// family opened on the paired owned-input impl immediately above.
/// Rust's `From` trait does not derive the `From<&Self>` sibling from
/// a `From<Self>` impl (the blanket
/// `impl<T, U> From<&T> for U where T: Clone, U: From<T>` does not
/// exist in `core`), so every newtype that carries the owned-input
/// reverse [`Cow<'static, str>`] axis but not the borrowed-input axis
/// forces every borrowed call site through a `.clone()` /
/// `<Cow<'static, str>>::from(v.clone())` detour whose type bounds
/// have no compile-time link back to the newtype.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_borrowed_into_owned_cow_str_routes_through_as_str_accessor`]
/// (byte-parity + [`Cow::Owned`]-arm pin against
/// [`CaixaVersion::as_str`] via a borrowed input, plus a
/// source-survival witness against silent move-out) and
/// [`tests::caixa_version_from_owned_and_borrowed_into_cow_str_agree_on_every_shape`]
/// (cross-axis partition pin against the paired owned-input impl on
/// the same instance, closing the "owned-input move vs. borrowed-input
/// clone" bifurcation on the same wrapped body through the
/// [`Cow<'static, str>`] axis).
impl From<&CaixaVersion> for std::borrow::Cow<'static, str> {
    fn from(v: &CaixaVersion) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Owned(v.as_str().to_owned())
    }
}

/// Trait-idiomatic *owned-input, [`Box<str>`] output* reverse projection
/// on the [`CaixaVersion`] newtype primitive — the [`Box<str>`] companion
/// to the paired owned-input [`From<CaixaVersion> for String`] (999a310)
/// and [`From<CaixaVersion> for std::borrow::Cow<'static, str>`] (55532e5)
/// impls on the same primitive. Routes through
/// [`String::into_boxed_str`]`(v.0)`, shrinking the wrapper's own heap
/// allocation to a fit-to-length boxed slice — no re-copy of the
/// per-instance version body's bytes on the fixed-capacity path
/// (`String::into_boxed_str` reuses the underlying `Vec<u8>` buffer
/// verbatim when the length matches its capacity; when the [`String`]
/// carries slack it reallocates once to shrink), so every consumer that
/// binds a [`CaixaVersion`] through the standard-library `.into()` /
/// [`From<Self> for Box<str>`] axis reaches the wrapped byte-string
/// through one substrate-primitive dispatch on the same underlying heap
/// storage the manifest-parse forward [`From<String> for CaixaVersion`]
/// constructor accepted.
///
/// A future consumer that wants a [`Box<str>`]-typed handle on a
/// [`CaixaVersion`] — a per-caixa struct field typed `Box<str>` rather
/// than [`String`] to trim the sixteen-byte length + capacity header
/// down to eight bytes on the pointer + length pair (a shape the
/// substrate acknowledges as the natural fixed-length storage for
/// once-written-never-mutated version strings held across the whole
/// operator reconciliation cycle), a future
/// `HashMap::<Box<str>, _>::from_iter([(versao.into(), _)])` per-versao
/// lookup where the map's key type is [`Box<str>`] rather than owned
/// [`String`] so the map's per-entry key-slot carries the sixteen-byte
/// [`Box<str>`] header instead of the twenty-four-byte [`String`]
/// header, a future M4 admission-webhook rejection body whose per-arm
/// error-frame composer accepts a [`Box<str>`] intermediate for the
/// same reason — reaches the wrapped byte-string through this one
/// dispatch, without the pre-lift `.to_string().into_boxed_str()`
/// double-hop that would allocate a fresh intermediary [`String`] on
/// the way to the same [`Box<str>`] slot.
///
/// Peer of the paired owned-input [`From<CaixaVersion> for String`]
/// (999a310) and [`From<CaixaVersion> for Cow<'static, str>`] (55532e5)
/// impls on the same primitive — all three route through `v.0`
/// (the [`String`] axis returns the wrapped buffer verbatim; the
/// [`Cow<'static, str>`] axis wraps it in [`Cow::Owned`]; this axis
/// shrinks it to a fit-to-length boxed slice via [`String::into_boxed_str`]),
/// preserving the zero-copy discipline the substrate opens on its
/// String-wrapper newtype primitive across the three reverse-projection
/// axes. Rust's standard library does not derive `From<Self> for
/// Box<str>` from `From<Self> for String` (nor from `From<Self> for
/// Cow<'static, str>`), so every newtype that carries the paired
/// reverse `From<Self> for String` axis but not the paired
/// [`Box<str>`] axis forces every [`Box<str>`]-typed call site through
/// a `.to_string().into_boxed_str()` double-allocation detour that
/// heap-allocates a fresh intermediary [`String`] between the wrapper
/// and the [`Box<str>`] slot.
///
/// Opens the trait-idiomatic *owned-input, [`Box<str>`]*
/// reverse-projection axis on the substrate's core String-wrapper
/// newtype primitive [`CaixaVersion`], extending the reverse-projection
/// matrix from the two axes already opened on this primitive (999a310
/// on the [`String`] axis, 55532e5 on the [`Cow<'static, str>`] axis)
/// onto the third. The fourth and final axis on the matrix
/// ([`std::sync::Arc<str>`]) is closed by the sibling paired
/// [`From<CaixaVersion> for std::sync::Arc<str>`] +
/// [`From<&CaixaVersion> for std::sync::Arc<str>`] impls immediately below.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_into_owned_box_str_returns_wrapped_body`]
/// (byte-parity pin against [`CaixaVersion::as_str`] on the same
/// instance, plus a round-trip witness through the paired
/// [`From<String> for CaixaVersion`] constructor closing the two-way
/// `Self → Box<str> → Self` cycle by construction) and
/// [`tests::caixa_version_from_into_owned_box_str_and_string_agree_on_every_shape`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaVersion> for String`] and
/// [`From<CaixaVersion> for Cow<'static, str>`] impls on the same
/// instance, closing the "owned-input into [`String`] vs. owned-input
/// into [`Cow<'static, str>`] vs. owned-input into [`Box<str>`]"
/// three-corner bifurcation on the same wrapped body).
impl From<CaixaVersion> for Box<str> {
    fn from(v: CaixaVersion) -> Box<str> {
        v.0.into_boxed_str()
    }
}

/// Trait-idiomatic *borrowed-input, [`Box<str>`] output* reverse
/// projection on the [`CaixaVersion`] newtype primitive — the
/// borrowed-input companion to the paired owned-input
/// [`From<CaixaVersion> for Box<str>`] impl immediately above. Routes
/// byte-for-byte through the substrate-primitive
/// [`CaixaVersion::as_str`] `pub const fn` accessor (via
/// [`Box::<str>::from`]`(&str)`, which allocates a fit-to-length boxed
/// slice from the borrowed `&str` in one heap allocation without an
/// intermediary [`String`]) so every consumer that holds a borrowed
/// [`&CaixaVersion`] and needs a [`Box<str>`] — a
/// `[…].iter().map(Box::<str>::from).collect::<Vec<_>>()` per-instance
/// materializer over `&[CaixaVersion]` (whose iterator yields
/// `&CaixaVersion`, not `CaixaVersion`, so the paired owned-input
/// [`From<CaixaVersion> for Box<str>`] axis alone forces every call
/// site through an explicit `.clone()` / dereference restatement), a
/// future `HashMap::<Box<str>, _>::from_iter` that keys off a
/// borrowed-iteration axis, a future generic
/// `<T: for<'a> Into<Box<str>>>`-bound emitter on a per-caixa
/// diagnostic column that walks the `iter().map(Into::into)` shape
/// verbatim — reaches the wrapped byte-string through this one dispatch
/// on the substrate primitive.
///
/// Second corner on the `{Self, &Self} → Box<str>` reverse-projection
/// family opened on the paired owned-input impl immediately above.
/// Rust's `From` trait does not derive the `From<&Self>` sibling from
/// a `From<Self>` impl (the blanket
/// `impl<T, U> From<&T> for U where T: Clone, U: From<T>` does not
/// exist in `core`), so every newtype that carries the owned-input
/// reverse [`Box<str>`] axis but not the borrowed-input axis forces
/// every borrowed call site through a `.clone()` /
/// `<Box<str>>::from(v.clone())` detour whose type bounds have no
/// compile-time link back to the newtype.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_borrowed_into_owned_box_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`] via a borrowed
/// input, plus a source-survival witness against silent move-out) and
/// [`tests::caixa_version_from_owned_and_borrowed_into_box_str_agree_on_every_shape`]
/// (cross-corner partition pin between owned-input move and
/// borrowed-input clone on the same wrapped body through the
/// [`Box<str>`] axis).
impl From<&CaixaVersion> for Box<str> {
    fn from(v: &CaixaVersion) -> Box<str> {
        Box::<str>::from(v.as_str())
    }
}

/// Trait-idiomatic *owned-input, [`std::sync::Arc<str>`] output* reverse
/// projection on the [`CaixaVersion`] newtype primitive — the
/// [`std::sync::Arc<str>`] companion to the paired owned-input
/// [`From<CaixaVersion> for String`] (999a310),
/// [`From<CaixaVersion> for std::borrow::Cow<'static, str>`] (55532e5),
/// and [`From<CaixaVersion> for Box<str>`] (32d861a) impls on the same
/// primitive. Routes through [`std::sync::Arc::<str>::from`]`(v.0)`,
/// which allocates a fresh atomically-refcounted heap slab whose data
/// slot byte-equals the wrapper's own [`String`] storage — the wrapped
/// bytes move through by value into the `Arc<str>` layout in one heap
/// allocation (the [`std::sync::Arc<str>`] layout carries a strong
/// count + weak count header ahead of the byte slice, so a copy is
/// required regardless of the input axis; no intermediary [`String`]
/// or [`Box<str>`] is materialized on the owned-input path).
///
/// A future consumer that wants a [`std::sync::Arc<str>`]-typed handle
/// on a [`CaixaVersion`] — a share-through-clone version body held
/// across a per-caixa `caixa-operator` reconcile task where every
/// spawn point wants a cheap `.clone()` on the version handle without
/// each task re-allocating its own [`String`] copy (the
/// [`std::sync::Arc::clone`] path bumps the atomic refcount in place
/// and returns a pointer-width handle), a future
/// `HashMap::<std::sync::Arc<str>, _>::from_iter` per-versao lookup
/// where the map's key type is [`std::sync::Arc<str>`] so the same
/// version-body pointer can key both the map and the payload without a
/// second heap allocation, a future M4 admission-webhook decoder that
/// materializes decoded version strings as [`std::sync::Arc<str>`]
/// slices so downstream verdict-composer tasks running on separate
/// worker threads can share the immutable body without a
/// per-consumer [`String::clone`] — reaches the wrapped byte-string
/// through this one dispatch, without the pre-lift
/// `.to_string().into::<std::sync::Arc<str>>()` double-hop that would
/// still allocate the same [`Arc<str>`] slab plus one intermediary
/// [`String`] between the wrapper and the [`std::sync::Arc<str>`] slot.
///
/// Peer of the paired owned-input [`From<CaixaVersion> for String`]
/// (999a310), [`From<CaixaVersion> for Cow<'static, str>`] (55532e5),
/// and [`From<CaixaVersion> for Box<str>`] (32d861a) impls on the same
/// primitive — all four route through `v.0` (the [`String`] axis
/// returns the wrapped buffer verbatim; the [`Cow<'static, str>`] axis
/// wraps it in [`Cow::Owned`]; the [`Box<str>`] axis shrinks it to a
/// fit-to-length boxed slice; this axis copies the bytes into a fresh
/// atomically-refcounted slab whose header carries the atomic strong +
/// weak counters the [`std::sync::Arc<str>`] layout requires),
/// preserving the substrate's single-dispatch reverse-projection
/// discipline across the four axes. Rust's standard library does not
/// derive `From<Self> for Arc<str>` from `From<Self> for String` (nor
/// from `From<Self> for Box<str>` or `From<Self> for Cow<'static, str>`),
/// so every newtype that carries the paired reverse `From<Self> for
/// String` / `Box<str>` / `Cow<'static, str>` axes but not the paired
/// [`std::sync::Arc<str>`] axis forces every
/// [`std::sync::Arc<str>`]-typed call site through a
/// `.to_string().into()` / `Arc::<str>::from(v.to_string())`
/// double-allocation detour that heap-allocates a fresh intermediary
/// [`String`] between the wrapper and the [`std::sync::Arc<str>`] slot.
///
/// Closes the trait-idiomatic *owned-input, [`std::sync::Arc<str>`]*
/// reverse-projection axis on the substrate's core String-wrapper
/// newtype primitive [`CaixaVersion`], completing the reverse-projection
/// matrix on this primitive across the full `{String, Cow<'static, str>,
/// Box<str>, Arc<str>}` roster — the fourth and final axis (999a310 on
/// the [`String`] axis, 55532e5 on the [`Cow<'static, str>`] axis,
/// 32d861a on the [`Box<str>`] axis, this axis on the
/// [`std::sync::Arc<str>`] axis).
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_into_owned_arc_str_returns_wrapped_body`]
/// (byte-parity pin against [`CaixaVersion::as_str`] on the same
/// instance, plus a round-trip witness through the paired
/// [`From<String> for CaixaVersion`] constructor closing the two-way
/// `Self → Arc<str> → Self` cycle by construction) and
/// [`tests::caixa_version_from_into_owned_arc_str_and_string_agree_on_every_shape`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaVersion> for String`], [`From<CaixaVersion> for Cow<'static, str>`],
/// and [`From<CaixaVersion> for Box<str>`] impls on the same instance,
/// closing the four-corner "owned-input into `String` vs. `Cow<'static, str>`
/// vs. `Box<str>` vs. `Arc<str>`" partition on the same wrapped body).
impl From<CaixaVersion> for std::sync::Arc<str> {
    fn from(v: CaixaVersion) -> std::sync::Arc<str> {
        std::sync::Arc::<str>::from(v.0)
    }
}

/// Trait-idiomatic *borrowed-input, [`std::sync::Arc<str>`] output*
/// reverse projection on the [`CaixaVersion`] newtype primitive — the
/// borrowed-input companion to the paired owned-input
/// [`From<CaixaVersion> for std::sync::Arc<str>`] impl immediately
/// above. Routes byte-for-byte through the substrate-primitive
/// [`CaixaVersion::as_str`] `pub const fn` accessor (via
/// [`std::sync::Arc::<str>::from`]`(&str)`, which allocates a fresh
/// atomically-refcounted heap slab from the borrowed `&str` in one
/// heap allocation without an intermediary [`String`] or [`Box<str>`])
/// so every consumer that holds a borrowed [`&CaixaVersion`] and needs
/// a [`std::sync::Arc<str>`] — a
/// `[…].iter().map(std::sync::Arc::<str>::from).collect::<Vec<_>>()`
/// per-instance materializer over `&[CaixaVersion]` (whose iterator
/// yields `&CaixaVersion`, not `CaixaVersion`, so the paired
/// owned-input [`From<CaixaVersion> for std::sync::Arc<str>`] axis
/// alone forces every call site through an explicit `.clone()` /
/// dereference restatement), a future
/// `HashMap::<std::sync::Arc<str>, _>::from_iter` that keys off a
/// borrowed-iteration axis, a future generic
/// `<T: for<'a> Into<std::sync::Arc<str>>>`-bound emitter on a
/// per-caixa diagnostic column that walks the
/// `iter().map(Into::into)` shape verbatim — reaches the wrapped
/// byte-string through this one dispatch on the substrate primitive.
///
/// Second corner on the `{Self, &Self} → std::sync::Arc<str>`
/// reverse-projection family opened on the paired owned-input impl
/// immediately above. Rust's `From` trait does not derive the
/// `From<&Self>` sibling from a `From<Self>` impl (the blanket
/// `impl<T, U> From<&T> for U where T: Clone, U: From<T>` does not
/// exist in `core`), so every newtype that carries the owned-input
/// reverse [`std::sync::Arc<str>`] axis but not the borrowed-input
/// axis forces every borrowed call site through a `.clone()` /
/// `<std::sync::Arc<str>>::from(v.clone())` detour whose type bounds
/// have no compile-time link back to the newtype.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_borrowed_into_owned_arc_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`] via a borrowed
/// input, plus a source-survival witness against silent move-out) and
/// [`tests::caixa_version_from_owned_and_borrowed_into_arc_str_agree_on_every_shape`]
/// (cross-corner partition pin between owned-input move and
/// borrowed-input clone on the same wrapped body through the
/// [`std::sync::Arc<str>`] axis).
impl From<&CaixaVersion> for std::sync::Arc<str> {
    fn from(v: &CaixaVersion) -> std::sync::Arc<str> {
        std::sync::Arc::<str>::from(v.as_str())
    }
}

/// Trait-idiomatic *owned-input, [`std::rc::Rc<str>`] output* reverse
/// projection on the [`CaixaVersion`] newtype primitive — the owned-heap-
/// string, single-threaded-reference-counted inverse of the pre-existing
/// [`From<String> for CaixaVersion`] / [`From<&str> for CaixaVersion`]
/// forward-projection pair on this primitive. Consumes the owned wrapper
/// by value, moves the wrapped [`String`] into the [`std::rc::Rc<str>`]
/// layout in one heap allocation (the [`std::rc::Rc<str>`] layout carries
/// a strong count + weak count header ahead of the byte slice, so the copy
/// is required regardless of the input axis; no intermediary [`String`] or
/// [`Box<str>`] is materialized on the owned-input path) — the exact
/// single-threaded mirror of the paired
/// [`From<CaixaVersion> for std::sync::Arc<str>`] impl (3e67756) on the
/// atomically-refcounted axis.
///
/// A future consumer that wants a [`std::rc::Rc<str>`]-typed handle on a
/// [`CaixaVersion`] — a per-`feira` verb's single-threaded diagnostic
/// composer that clones the version body across a chain of Nord-themed
/// column emitters without paying either the [`String::clone`]
/// full-allocation cost (every step re-allocates its own buffer) or the
/// atomic-refcount overhead the paired [`std::sync::Arc<str>`] axis
/// forces (the [`std::rc::Rc::clone`] path bumps a non-atomic refcount in
/// place and returns a pointer-width handle, cheaper than the paired
/// atomic increment on the sibling [`std::sync::Arc<str>`] axis by a
/// measurable margin on hot single-threaded call sites), a future single-
/// threaded `HashMap::<std::rc::Rc<str>, _>::from_iter` per-versao lookup
/// where the map's key type is [`std::rc::Rc<str>`] so the same version-
/// body pointer can key both the map and the payload without a second heap
/// allocation, a future `feira lint` per-caixa diagnostic table whose
/// per-column `Cell<std::rc::Rc<str>>` payload carries the version body
/// across the row-composer + column-composer + wrapper phases through the
/// pointer-width handle rather than a [`String`] per phase — reaches the
/// wrapped byte-string through this one dispatch, without the pre-lift
/// `.to_string().into::<std::rc::Rc<str>>()` double-hop that would still
/// allocate the same [`Rc<str>`] slab plus one intermediary [`String`]
/// between the wrapper and the [`std::rc::Rc<str>`] slot.
///
/// Peer of the paired owned-input [`From<CaixaVersion> for String`]
/// (999a310), [`From<CaixaVersion> for Cow<'static, str>`] (55532e5),
/// [`From<CaixaVersion> for Box<str>`] (32d861a), and
/// [`From<CaixaVersion> for std::sync::Arc<str>`] (3e67756) impls on the
/// same primitive — all five route through `v.0` (the [`String`] axis
/// returns the wrapped buffer verbatim; the [`Cow<'static, str>`] axis
/// wraps it in [`Cow::Owned`]; the [`Box<str>`] axis shrinks it to a
/// fit-to-length boxed slice; the [`Arc<str>`] axis copies the bytes into
/// a fresh atomically-refcounted slab; this axis copies the bytes into a
/// fresh single-threaded-refcounted slab whose header carries the non-
/// atomic strong + weak counters the [`std::rc::Rc<str>`] layout
/// requires), preserving the substrate's single-dispatch reverse-
/// projection discipline across the five axes. Rust's standard library
/// does not derive `From<Self> for Rc<str>` from `From<Self> for Arc<str>`
/// (the [`std::sync::Arc<str>`] and [`std::rc::Rc<str>`] layouts share the
/// same on-disk shape but the trait tables are disjoint, and no blanket
/// `impl<T> From<T> for Rc<str> where Arc<str>: From<T>` exists in
/// `core`), so every newtype that carries the paired
/// [`std::sync::Arc<str>`] axis but not the paired [`std::rc::Rc<str>`]
/// axis forces every single-threaded [`std::rc::Rc<str>`]-typed call site
/// through a `.to_string().into()` / `Rc::<str>::from(v.to_string())`
/// double-allocation detour that heap-allocates a fresh intermediary
/// [`String`] between the wrapper and the [`std::rc::Rc<str>`] slot.
///
/// Extends the trait-idiomatic *owned-input* reverse-projection matrix on
/// the substrate's core String-wrapper newtype primitive [`CaixaVersion`]
/// onto the single-threaded reference-counted axis — the fifth axis
/// (999a310 on [`String`], 55532e5 on [`Cow<'static, str>`], 32d861a on
/// [`Box<str>`], 3e67756 on [`std::sync::Arc<str>`], this axis on
/// [`std::rc::Rc<str>`]).
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_into_owned_rc_str_returns_wrapped_body`]
/// (byte-parity pin against [`CaixaVersion::as_str`] on the same instance,
/// plus a round-trip witness through the paired [`From<String> for
/// CaixaVersion`] constructor closing the two-way `Self → Rc<str> → Self`
/// cycle by construction) and
/// [`tests::caixa_version_from_into_owned_rc_str_and_arc_str_agree_on_every_shape`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaVersion> for String`],
/// [`From<CaixaVersion> for Cow<'static, str>`],
/// [`From<CaixaVersion> for Box<str>`], and
/// [`From<CaixaVersion> for std::sync::Arc<str>`] impls on the same
/// instance, closing the five-corner "owned-input into `String` vs.
/// `Cow<'static, str>` vs. `Box<str>` vs. `Arc<str>` vs. `Rc<str>`"
/// partition on the same wrapped body).
impl From<CaixaVersion> for std::rc::Rc<str> {
    fn from(v: CaixaVersion) -> std::rc::Rc<str> {
        std::rc::Rc::<str>::from(v.0)
    }
}

/// Trait-idiomatic *borrowed-input, [`std::rc::Rc<str>`] output* reverse
/// projection on the [`CaixaVersion`] newtype primitive — the borrowed-
/// input companion to the paired owned-input
/// [`From<CaixaVersion> for std::rc::Rc<str>`] impl immediately above.
/// Routes byte-for-byte through the substrate-primitive
/// [`CaixaVersion::as_str`] `pub const fn` accessor (via
/// [`std::rc::Rc::<str>::from`]`(&str)`, which allocates a fresh
/// single-threaded-refcounted heap slab from the borrowed `&str` in one
/// heap allocation without an intermediary [`String`] or [`Box<str>`]) so
/// every consumer that holds a borrowed [`&CaixaVersion`] and needs a
/// [`std::rc::Rc<str>`] — a
/// `[…].iter().map(std::rc::Rc::<str>::from).collect::<Vec<_>>()`
/// per-instance materializer over `&[CaixaVersion]` (whose iterator yields
/// `&CaixaVersion`, not `CaixaVersion`, so the paired owned-input
/// [`From<CaixaVersion> for std::rc::Rc<str>`] axis alone forces every
/// call site through an explicit `.clone()` / dereference restatement), a
/// future single-threaded `HashMap::<std::rc::Rc<str>, _>::from_iter` that
/// keys off a borrowed-iteration axis, a future generic
/// `<T: for<'a> Into<std::rc::Rc<str>>>`-bound emitter on a per-caixa
/// diagnostic column that walks the `iter().map(Into::into)` shape
/// verbatim — reaches the wrapped byte-string through this one dispatch on
/// the substrate primitive.
///
/// Second corner on the `{Self, &Self} → std::rc::Rc<str>` reverse-
/// projection family opened on the paired owned-input impl immediately
/// above. Rust's `From` trait does not derive the `From<&Self>` sibling
/// from a `From<Self>` impl (the blanket `impl<T, U> From<&T> for U where
/// T: Clone, U: From<T>` does not exist in `core`), so every newtype that
/// carries the owned-input reverse [`std::rc::Rc<str>`] axis but not the
/// borrowed-input axis forces every borrowed call site through a
/// `.clone()` / `<std::rc::Rc<str>>::from(v.clone())` detour whose type
/// bounds have no compile-time link back to the newtype.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_borrowed_into_owned_rc_str_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`] via a borrowed input,
/// plus a source-survival witness against silent move-out) and
/// [`tests::caixa_version_from_owned_and_borrowed_into_rc_str_agree_on_every_shape`]
/// (cross-corner partition pin between owned-input move and borrowed-
/// input clone on the same wrapped body through the [`std::rc::Rc<str>`]
/// axis).
impl From<&CaixaVersion> for std::rc::Rc<str> {
    fn from(v: &CaixaVersion) -> std::rc::Rc<str> {
        std::rc::Rc::<str>::from(v.as_str())
    }
}

/// Trait-idiomatic *byte-view* borrow projection on the [`CaixaVersion`]
/// newtype primitive — the byte-view mirror of the pre-existing sibling
/// [`AsRef<str>`] str-view borrow projection on this same primitive.
/// Routes byte-for-byte through the substrate-primitive
/// [`CaixaVersion::as_str`] `pub const fn` accessor via [`str::as_bytes`]
/// so every consumer that binds a [`CaixaVersion`] through the standard-
/// library `impl AsRef<[u8]>` bound reaches the wrapped [`String`]'s
/// byte-tail through one substrate-primitive dispatch rather than through
/// the pre-lift open-coded `v.as_str().as_bytes()` /
/// `<CaixaVersion as AsRef<str>>::as_ref(&v).as_bytes()` two-hop
/// composition whose bounds carry no compile-time link back to the
/// newtype's storage.
///
/// The primary compounding target is the [`crate`]-adjacent
/// [`caixa-lacre`](../../caixa-lacre/) BLAKE3 content-address closure:
/// [`blake3::hash`] and [`blake3::Hasher::update`] both bind their input
/// through `impl AsRef<[u8]>`, so any future per-caixa content-address
/// tag that folds a `:versao` byte-tail into the [`crate::Lacre`] closure
/// (a hypothetical `hasher.update(caixa.versao());`-shape composition on
/// the per-caixa BLAKE3 closure builder, a future per-`Membro :versao`
/// requirement fold on the M3-mesh lacre snapshot the operator's
/// admission cycle pins each Aplicacao's `:membros :versao` accept-set
/// against, a future per-`ChildSpec :versao-requirement` fold on the
/// M2-OTP-shape supervisor-tree lacre snapshot) reaches the substrate-
/// primitive [`CaixaVersion::as_str`] accessor through this impl and no
/// other. Peer consumer paths on the byte-view axis: any future
/// [`std::io::Write::write_all`]-bound diagnostic sink (whose input binds
/// through `impl AsRef<[u8]>`), any future byte-keyed
/// [`std::collections::HashMap`] `<K: AsRef<[u8]>, V>` lookup whose entry-
/// key trait bound rules out the sibling [`AsRef<str>`] str-view
/// projection, and any future `ring::digest::Context::update` /
/// `sha2::Sha256::update` / `blake3::Hasher::update` byte-input surface
/// on any future per-`:versao` content-address digest.
///
/// Rust's standard library carries `impl AsRef<[u8]> for str` and
/// `impl AsRef<[u8]> for String`, so the two-hop composition
/// `v.as_str().as_bytes()` (equivalently
/// `AsRef::<str>::as_ref(&v).as_bytes()`) is reachable through the pre-
/// existing str-view axis alone. But that two-hop shape has no compile-
/// time link back to the byte-projection axis, forces every downstream
/// `<T: AsRef<[u8]>>`-bound consumer to open-code the two-hop composition
/// at every call site rather than pass a [`CaixaVersion`] through the
/// trait bound directly, and admits a silent split whenever a future call
/// site takes a sibling reverse-projection axis (a `String::from(v)`
/// unwrap, a `Box::<str>::from(&v)` fit-to-length boxed slice, a
/// `Cow::<'static, str>::from(v)` owned-arm wrap) whose `.as_bytes()`
/// byte-tail byte-equals `as_str`'s by construction but carries no
/// compile-time byte-view surface. The lifted single-hop impl closes the
/// byte-view axis so every future `<T: AsRef<[u8]>>`-bound consumer
/// reaches the substrate primitive through one trait dispatch, and any
/// future rebrand of the wrapped storage (a hypothetical widening to a
/// typed [`semver::Version`] slot once eager parse-on-construct
/// discipline lands) migrates the byte-view surface in lockstep with the
/// paired [`AsRef<str>`] / [`fmt::Display`] / [`std::borrow::Borrow<str>`]
/// projections at the shared [`CaixaVersion::as_str`] accessor.
///
/// Rust-side newtype convention pairs [`AsRef<str>`] and [`AsRef<[u8]>`]
/// on the same primitive (the standard library's own [`String`] carries
/// both — `impl AsRef<str> for String` + `impl AsRef<[u8]> for String` —
/// on the same borrow-projection axis), so a newtype that carries one but
/// not the other splits off the convention that lets every
/// [`String`]-shaped consumer swap the newtype in without re-shaping its
/// bounds. This impl closes that split on [`CaixaVersion`], mirroring the
/// paired [`AsRef<[u8]>`] byte-view axis every closed-set fieldless typed
/// enum peer on the substrate already carries
/// ([`crate::CaixaKind`] at kind.rs:1791, [`crate::CaixaDialeto`] at
/// dialeto.rs:1885, [`crate::dep::DepList`] at dep.rs:4616,
/// [`crate::supervisor::RestartStrategy`] at supervisor.rs:1518,
/// [`crate::supervisor::RestartPolicy`] at supervisor.rs:3385,
/// [`crate::aplicacao::WitShape`] at aplicacao.rs:1955,
/// [`crate::aplicacao::RateLimitUnit`] at aplicacao.rs:8900,
/// [`crate::aplicacao::PlacementStrategy`] at aplicacao.rs:12231), now
/// extended onto the substrate's core String-wrapper newtype primitive.
/// The str-view axes stay reachable for the borrowed `&str` and
/// formatter-output paths, this impl closes the byte-view axis at the
/// same shared substrate-primitive accessor.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_as_ref_bytes_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`] `.as_bytes()`, plus
/// cross-axis witness against the paired [`AsRef<str>`] and
/// [`fmt::Display`] axes' `.as_bytes()` byte-tails, plus a
/// `<T: AsRef<[u8]>>`-bound-consumer witness that a generic byte-input
/// function accepts a [`CaixaVersion`] directly through the trait bound
/// and reaches the wrapped body without the caller open-coding the
/// two-hop projection). Any future silent detour that routes the byte-
/// view impl off the substrate-primitive [`CaixaVersion::as_str`]
/// accessor (a swap onto `self.0.as_bytes()` re-inlining that bypasses
/// the shared `pub const fn` dispatch, a stray normalization step that
/// would drop whitespace or canonicalize a prerelease tag ahead of the
/// byte-view emit, a swap onto a hypothetical future [`semver::Version`]
/// re-serialization that would strip the raw at-rest storage discipline
/// [`CaixaVersion`] carries by construction) trips at caixa-core test
/// time under `assert_eq!` rather than at a downstream `impl AsRef<[u8]>`-
/// bound consumer's silent split.
impl AsRef<[u8]> for CaixaVersion {
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}

/// Trait-idiomatic *owned-input, owned-[`Vec<u8>`] output* byte-owned
/// forward projection on the [`CaixaVersion`] newtype primitive — the
/// byte-mirror of the pre-existing owned-input [`From<CaixaVersion> for
/// String`] str-owned forward-projection axis and the owned-heap peer of
/// the borrow-projection [`AsRef<[u8]>`] byte-view axis (the prior lift on
/// this same primitive) that only surfaces a `&[u8]` view without an
/// owned-heap byte-tail.
///
/// Routes the wrapped [`String`] storage through
/// [`String::into_bytes`] verbatim ([`Self::0`], a move of the pre-existing
/// heap allocation reused byte-for-byte as the [`Vec<u8>`] backing buffer —
/// no re-copy of the per-instance version body's bytes, no allocating
/// detour through `.as_str().as_bytes().to_vec()`), so every consumer
/// that binds a [`CaixaVersion`] through the standard-library `.into()` /
/// [`From<Self> for Vec<u8>`] (equivalently [`Into<Vec<u8>>`]) axis
/// reaches the wrapped byte-string through one substrate-primitive
/// dispatch on the exact same heap allocation the manifest-parse forward
/// [`From<String> for CaixaVersion`] constructor accepted.
///
/// A future consumer that wants an owned [`Vec<u8>`] byte-tail on a
/// [`CaixaVersion`] — a future
/// [`std::io::Write::write_all`]-shape per-caixa `:versao` audit-log
/// byte-sink whose input parameter is an owned [`Vec<u8>`] payload, a
/// future `bytes::Bytes::from(Vec::<u8>::from(v))` composer folding the
/// canonical `SemVer` wire form into a [`bytes::Bytes`] framing surface,
/// a future `hasher.update(&Vec::<u8>::from(v))`-shape BLAKE3 content-
/// address closure that needs the owned byte-tail buffered before folding
/// into the per-caixa [`crate::Lacre`] closure body, a future per-caixa
/// protobuf/CBOR/msgpack payload composer whose framer takes an owned
/// [`Vec<u8>`] rather than a borrowed byte-slice, a future M4 admission-
/// webhook rejection body composer that emits the offending
/// [`CaixaVersion`] as a raw byte-tail through an
/// [`std::io::Write`]-shape sink — reaches the substrate primitive
/// through one trait dispatch, avoiding the pre-lift open-coded
/// `v.as_str().as_bytes().to_vec()` two-hop composition (a fresh heap
/// allocation copied byte-by-byte off the wrapper's own storage) or the
/// `String::from(v).into_bytes()` two-hop reverse-then-move shape whose
/// intermediate `String` step carries no compile-time link back to the
/// byte-view axis.
///
/// Peer of the pre-existing byte-view [`AsRef<[u8]>`] impl on the same
/// primitive — both project the wrapped [`String`] onto its byte-tail,
/// but the [`AsRef<[u8]>`] axis surfaces a borrowed `&[u8]` view for
/// consumers that never take ownership while this impl surfaces an owned
/// [`Vec<u8>`] for consumers whose framing / sink / hasher / channel
/// surfaces demand a heap-owned buffer. Rust's standard library
/// deliberately splits the two axes on the two bounds they carry
/// (`impl AsRef<[u8]> for String` for the borrowed-view surface,
/// `impl From<String> for Vec<u8>` via [`String::into_bytes`] for the
/// owned-heap surface), so a newtype that carries one but not the other
/// splits off the convention that lets every [`String`]-shaped consumer
/// swap the newtype in without re-shaping its bounds.
///
/// Opens the trait-idiomatic *owned-input, [`Vec<u8>`]* byte-owned
/// forward-projection axis on the substrate's core String-wrapper newtype
/// primitive [`CaixaVersion`], mirroring the paired
/// `From<{Self, &Self}> for Vec<u8>` axis every sibling closed-set
/// fieldless typed enum peer on the substrate already carries
/// ([`crate::CaixaKind`], [`crate::CaixaDialeto`], [`crate::dep::DepList`],
/// [`crate::supervisor::RestartStrategy`],
/// [`crate::supervisor::RestartPolicy`], [`crate::aplicacao::WitShape`],
/// [`crate::aplicacao::RateLimitUnit`],
/// [`crate::aplicacao::PlacementStrategy`], and the compound sibling
/// [`crate::aplicacao::RateLimit`] at aplicacao.rs:6743 which routes
/// through its paired [`fmt::Display`] via `.to_string().into_bytes()`),
/// now extended onto the substrate's core String-wrapper newtype
/// primitive.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_into_owned_vec_bytes_returns_wrapped_body`]
/// (byte-parity pin against [`CaixaVersion::as_str`] `.as_bytes()` on the
/// same instance, plus a round-trip witness through the paired
/// [`From<String> for CaixaVersion`] constructor after re-materializing
/// the [`Vec<u8>`] through [`String::from_utf8`]) and
/// [`tests::caixa_version_from_owned_and_borrowed_into_vec_bytes_agree_on_every_shape`]
/// (cross-axis partition pin against the paired borrowed-input impl and
/// against the pre-existing byte-view [`AsRef<[u8]>`] axis on the same
/// instance, closing the "owned-input move vs. borrowed-input clone" and
/// "owned-heap vs. borrowed-view" bifurcations on the same wrapped body).
impl From<CaixaVersion> for Vec<u8> {
    fn from(v: CaixaVersion) -> Vec<u8> {
        v.0.into_bytes()
    }
}

/// Trait-idiomatic *borrowed-input, owned-[`Vec<u8>`] output* byte-owned
/// forward projection on the [`CaixaVersion`] newtype primitive — the
/// borrowed-input companion to the paired owned-input
/// [`From<CaixaVersion> for Vec<u8>`] impl immediately above. Routes
/// byte-for-byte through the substrate-primitive
/// [`CaixaVersion::as_str`] `pub const fn` accessor (via
/// [`str::as_bytes`] + [`slice::to_vec`]) so every consumer that holds a
/// borrowed [`&CaixaVersion`] and needs an owned [`Vec<u8>`] — a
/// `[…].iter().map(Vec::<u8>::from).collect::<Vec<_>>()` per-instance
/// materializer over `&[CaixaVersion]` (whose iterator yields
/// `&CaixaVersion`, not `CaixaVersion`, so the owned-input
/// [`From<CaixaVersion> for Vec<u8>`] axis alone forces every call site
/// through an explicit `.clone()` restatement whose intermediate
/// [`CaixaVersion`] allocation is discarded on the very next call), a
/// future admission-webhook diagnostic body composer that walks a
/// `&Vec<CaixaVersion>` overlay through an [`Into<Vec<u8>>`]-bound per-
/// arm byte-writer to surface each offending `:versao` wire form, a
/// future per-`Membro :versao`-shape audit-log byte-writer that folds
/// each Aplicacao's member-versao byte-tail into an owned [`Vec<u8>`]
/// sink without moving out of the borrowed [`AplicacaoSpec::membros`]
/// slice — reaches the wrapped byte-string through this one dispatch on
/// the substrate primitive.
///
/// Second corner on the `{Self, &Self} → Vec<u8>` byte-owned forward-
/// projection family opened on the paired owned-input
/// [`From<CaixaVersion> for Vec<u8>`] impl immediately above. Rust's
/// `From` trait does not derive the `From<&Self>` sibling from a
/// `From<Self>` impl (the blanket
/// `impl<T, U> From<&T> for U where T: Clone, U: From<T>` does not exist
/// in `core`), so every newtype that carries the owned-input byte-owned
/// forward axis but not the borrowed-input axis forces every borrowed
/// call site through a `.clone()` / `<Vec<u8>>::from(v.clone())` detour
/// whose type bounds have no compile-time link back to the newtype.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_borrowed_into_owned_vec_bytes_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`] `.as_bytes()` via a
/// borrowed input, plus a source-survival witness against silent move-
/// out) and
/// [`tests::caixa_version_from_owned_and_borrowed_into_vec_bytes_agree_on_every_shape`]
/// (cross-corner partition pin between owned-input move and borrowed-
/// input clone on the same wrapped body through the [`Vec<u8>`] axis).
impl From<&CaixaVersion> for Vec<u8> {
    fn from(v: &CaixaVersion) -> Vec<u8> {
        v.as_str().as_bytes().to_vec()
    }
}

/// Trait-idiomatic *owned-input, [`std::borrow::Cow<'static, [u8]>`]
/// output* byte-owned reverse projection on the [`CaixaVersion`] newtype
/// primitive — the [`Cow<'static, [u8]>`] companion to the paired owned-
/// input [`From<CaixaVersion> for Vec<u8>`] impl (98d38ed) on the same
/// primitive, mirroring the sibling str-family [`From<CaixaVersion> for
/// std::borrow::Cow<'static, str>`] axis (55532e5) onto the byte-family
/// side of the reverse-projection matrix. Routes through
/// [`std::borrow::Cow::Owned`]`(v.0.into_bytes())`, moving the wrapper's
/// own heap allocation through [`String::into_bytes`] verbatim — no
/// re-copy of the per-instance version body's bytes on the owned-input
/// path, no allocating detour through [`CaixaVersion::as_str`] +
/// [`slice::to_vec`], no mis-routing onto [`Cow::Borrowed`] on a
/// runtime byte-string that cannot promise the `&'static [u8]` lifetime
/// the borrowed arm requires.
///
/// A future consumer that wants a [`Cow<'static, [u8]>`]-typed handle
/// on a [`CaixaVersion`] — a
/// `tracing::field::Value::Bytes(caixa.versao.into())`-shape structured-
/// log key on a future per-caixa `caixa-operator` reconciliation counter
/// (whose byte-tail emit surface types byte-keys as
/// [`Cow<'static, [u8]>`] so static compile-time literals and dynamic
/// version bodies share the same key-slot without an unconditional heap
/// allocation on the literal path), a future
/// `HashMap::<Cow<'static, [u8]>, _>::from_iter([(versao.into(), _)])`
/// per-versao lookup where the map's key type is [`Cow<'static, [u8]>`]
/// rather than owned [`Vec<u8>`] so literal-lifetime byte-keys can share
/// the same map without wrapping in an extra [`Vec<u8>`] allocation, a
/// future M4 admission-webhook rejection body whose per-arm error-frame
/// composer accepts a [`Cow<'static, [u8]>`] intermediate for the same
/// reason — reaches the wrapped byte-string through this one dispatch,
/// without the pre-lift `Vec::<u8>::from(v).into()` /
/// `Cow::Owned(Vec::from(v))` double-hop that would still allocate the
/// same [`Cow::Owned`] arm through the sibling byte-owned axis.
///
/// Deliberately returns [`std::borrow::Cow::Owned`] rather than
/// [`std::borrow::Cow::Borrowed`] — the substrate-primitive
/// [`CaixaVersion::as_str`] accessor's `.as_bytes()` return does not
/// carry the `&'static [u8]` lifetime by construction (the wrapped
/// [`String`] storage is a runtime heap allocation, not a compile-time
/// literal), so the [`Cow<'static, [u8]>`] output shape rules out the
/// borrowed arm and the owned arm is the type-correct projection
/// (mirroring the paired sibling [`From<CaixaVersion> for
/// std::borrow::Cow<'static, str>`] axis's own [`Cow::Owned`] discipline
/// at 55532e5). Peer of the paired owned-input
/// [`From<CaixaVersion> for Vec<u8>`] impl (98d38ed) on the same
/// primitive — both route through the wrapper's own heap allocation via
/// a move on `v.0.into_bytes()`, preserving the zero-copy discipline the
/// substrate opens on its String-wrapper newtype primitive across the
/// byte-family reverse-projection matrix.
///
/// Extends the trait-idiomatic *owned-input* byte-family reverse-
/// projection matrix on the substrate's core String-wrapper newtype
/// primitive [`CaixaVersion`] onto the [`Cow<'static, [u8]>`] axis —
/// the second corner (98d38ed on [`Vec<u8>`], this axis on
/// [`Cow<'static, [u8]>`]) on the byte-family side, mirroring the
/// paired str-family axis at 55532e5 on [`Cow<'static, str>`]. Rust's
/// standard library does not derive `From<Self> for Cow<'static, [u8]>`
/// from `From<Self> for Vec<u8>` (nor from `From<Self> for Cow<'static,
/// str>`), so every newtype that carries a byte-owned [`Vec<u8>`]
/// reverse axis but not the paired [`Cow<'static, [u8]>`] axis forces
/// every [`Cow<'static, [u8]>`]-typed call site through a
/// `Vec::<u8>::from(v).into()` intermediary allocation whose bounds
/// carry no compile-time link back to the newtype's storage.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_into_owned_cow_bytes_returns_owned_wrapped_body`]
/// (byte-parity + [`Cow::Owned`]-arm pin against
/// [`CaixaVersion::as_str`] `.as_bytes()` on the same instance, plus a
/// round-trip witness through the paired [`From<String> for
/// CaixaVersion`] constructor after re-materializing the byte-tail
/// through [`String::from_utf8`]) and
/// [`tests::caixa_version_from_into_owned_cow_bytes_and_vec_bytes_agree_on_every_shape`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaVersion> for Vec<u8>`] and pre-existing byte-view
/// [`AsRef<[u8]>`] impls on the same instance, closing the "owned-input
/// into [`Vec<u8>`] vs. owned-input into [`Cow<'static, [u8]>`]"
/// bifurcation on the same wrapped body).
impl From<CaixaVersion> for std::borrow::Cow<'static, [u8]> {
    fn from(v: CaixaVersion) -> std::borrow::Cow<'static, [u8]> {
        std::borrow::Cow::Owned(v.0.into_bytes())
    }
}

/// Trait-idiomatic *borrowed-input, [`std::borrow::Cow<'static, [u8]>`]
/// output* byte-owned reverse projection on the [`CaixaVersion`] newtype
/// primitive — the borrowed-input companion to the paired owned-input
/// [`From<CaixaVersion> for std::borrow::Cow<'static, [u8]>`] impl
/// immediately above. Routes byte-for-byte through the substrate-
/// primitive [`CaixaVersion::as_str`] `pub const fn` accessor (via
/// [`str::as_bytes`] + [`slice::to_vec`] wrapped in
/// [`std::borrow::Cow::Owned`]) so every consumer that holds a borrowed
/// [`&CaixaVersion`] and needs a [`Cow<'static, [u8]>`] — a
/// `[…].iter().map(Cow::<'static, [u8]>::from).collect::<Vec<_>>()`
/// per-instance materializer over `&[CaixaVersion]` (whose iterator
/// yields `&CaixaVersion`, not `CaixaVersion`, so the paired owned-input
/// [`From<CaixaVersion> for Cow<'static, [u8]>`] axis alone forces every
/// call site through an explicit `.clone()` / dereference restatement),
/// a future `HashMap::<Cow<'static, [u8]>, _>::from_iter` that keys off
/// a borrowed-iteration axis where cloning the wrapper would allocate
/// one [`String`] beyond the eventual [`Cow::Owned`] arm's own, a future
/// generic `<T: for<'a> Into<Cow<'static, [u8]>>>`-bound byte-writer on
/// a per-caixa diagnostic column that walks the
/// `iter().map(Into::into)` shape verbatim — reaches the wrapped
/// byte-string through this one dispatch on the substrate primitive.
///
/// Deliberately returns [`std::borrow::Cow::Owned`] rather than
/// [`std::borrow::Cow::Borrowed`] — the substrate-primitive
/// [`CaixaVersion::as_str`] accessor's `.as_bytes()` return does not
/// carry the `&'static [u8]` lifetime by construction, so the
/// [`Cow<'static, [u8]>`] output shape rules out the borrowed arm and
/// the owned arm is the type-correct projection (mirroring the paired
/// owned-input impl's own [`Cow::Owned`] discipline). Second corner on
/// the `{Self, &Self} → Cow<'static, [u8]>` byte-owned reverse-
/// projection family opened on the paired owned-input impl immediately
/// above. Rust's `From` trait does not derive the `From<&Self>` sibling
/// from a `From<Self>` impl (the blanket
/// `impl<T, U> From<&T> for U where T: Clone, U: From<T>` does not exist
/// in `core`), so every newtype that carries the owned-input byte-owned
/// reverse [`Cow<'static, [u8]>`] axis but not the borrowed-input axis
/// forces every borrowed call site through a `.clone()` /
/// `<Cow<'static, [u8]>>::from(v.clone())` detour whose type bounds
/// have no compile-time link back to the newtype.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_borrowed_into_owned_cow_bytes_routes_through_as_str_accessor`]
/// (byte-parity + [`Cow::Owned`]-arm pin against
/// [`CaixaVersion::as_str`] `.as_bytes()` via a borrowed input, plus a
/// source-survival witness against silent move-out) and
/// [`tests::caixa_version_from_owned_and_borrowed_into_cow_bytes_agree_on_every_shape`]
/// (cross-corner partition pin between owned-input move and borrowed-
/// input clone on the same wrapped body through the [`Cow<'static,
/// [u8]>`] axis).
impl From<&CaixaVersion> for std::borrow::Cow<'static, [u8]> {
    fn from(v: &CaixaVersion) -> std::borrow::Cow<'static, [u8]> {
        std::borrow::Cow::Owned(v.as_str().as_bytes().to_vec())
    }
}

/// Trait-idiomatic *owned-input, [`Box<[u8]>`] output* byte-owned reverse
/// projection on the [`CaixaVersion`] newtype primitive — the [`Box<[u8]>`]
/// companion to the paired owned-input [`From<CaixaVersion> for Vec<u8>`]
/// (98d38ed) and [`From<CaixaVersion> for std::borrow::Cow<'static, [u8]>`]
/// (baf7537) impls on the same primitive, mirroring the sibling str-family
/// [`From<CaixaVersion> for Box<str>`] axis (32d861a) onto the byte-family
/// side of the reverse-projection matrix. Routes through
/// [`Vec::<u8>::into_boxed_slice`]`(v.0.into_bytes())`, moving the wrapper's
/// own heap allocation into a fit-to-length boxed byte slice — no re-copy of
/// the per-instance version body's bytes on the fixed-capacity path
/// ([`Vec::<u8>::into_boxed_slice`] reuses the underlying buffer verbatim
/// when the length matches its capacity; when the [`Vec<u8>`] carries slack
/// it reallocates once to shrink), so every consumer that binds a
/// [`CaixaVersion`] through the standard-library `.into()` /
/// [`From<Self> for Box<[u8]>`] axis reaches the wrapped byte-string through
/// one substrate-primitive dispatch on the same underlying heap storage the
/// manifest-parse forward [`From<String> for CaixaVersion`] constructor
/// accepted.
///
/// A future consumer that wants a [`Box<[u8]>`]-typed handle on a
/// [`CaixaVersion`] — a per-caixa struct field typed `Box<[u8]>` rather than
/// [`Vec<u8>`] to trim the twenty-four-byte pointer + length + capacity
/// header down to sixteen bytes on the pointer + length pair (a shape the
/// substrate acknowledges as the natural fixed-length storage for
/// once-written-never-mutated version byte-tails held across the whole
/// operator reconciliation cycle), a future
/// `HashMap::<Box<[u8]>, _>::from_iter([(versao.into(), _)])` per-versao
/// lookup where the map's key type is [`Box<[u8]>`] rather than owned
/// [`Vec<u8>`] so the map's per-entry key-slot carries the sixteen-byte
/// [`Box<[u8]>`] header instead of the twenty-four-byte [`Vec<u8>`] header,
/// a future M4 admission-webhook rejection body whose per-arm error-frame
/// composer accepts a [`Box<[u8]>`] intermediate for the same reason —
/// reaches the wrapped byte-string through this one dispatch, without the
/// pre-lift `Vec::<u8>::from(v).into_boxed_slice()` double-hop that would
/// still allocate through the same [`Vec<u8>`] intermediary on the way to
/// the same [`Box<[u8]>`] slot but with one extra header-slot round-trip.
///
/// Peer of the paired owned-input [`From<CaixaVersion> for Vec<u8>`]
/// (98d38ed) and [`From<CaixaVersion> for std::borrow::Cow<'static, [u8]>`]
/// (baf7537) impls on the same primitive — all three route through
/// `v.0.into_bytes()` (the [`Vec<u8>`] axis returns the wrapped buffer
/// verbatim; the [`Cow<'static, [u8]>`] axis wraps it in [`Cow::Owned`];
/// this axis shrinks it to a fit-to-length boxed byte slice via
/// [`Vec::<u8>::into_boxed_slice`]), preserving the zero-copy discipline
/// the substrate opens on its String-wrapper newtype primitive across the
/// byte-family reverse-projection matrix. Rust's standard library does not
/// derive `From<Self> for Box<[u8]>` from `From<Self> for Vec<u8>` (nor
/// from `From<Self> for Cow<'static, [u8]>`), so every newtype that carries
/// the paired reverse [`Vec<u8>`] axis but not the paired [`Box<[u8]>`] axis
/// forces every [`Box<[u8]>`]-typed call site through a
/// `Vec::<u8>::from(v).into_boxed_slice()` intermediary allocation whose
/// bounds carry no compile-time link back to the newtype's storage.
///
/// Extends the trait-idiomatic *owned-input* byte-family reverse-projection
/// matrix on the substrate's core String-wrapper newtype primitive
/// [`CaixaVersion`] onto the [`Box<[u8]>`] axis — the third corner (98d38ed
/// on [`Vec<u8>`], baf7537 on [`Cow<'static, [u8]>`], this axis on
/// [`Box<[u8]>`]) on the byte-family side, mirroring the paired str-family
/// axis at 32d861a on [`Box<str>`].
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_into_owned_box_bytes_returns_wrapped_body`]
/// (byte-parity pin against [`CaixaVersion::as_str`] `.as_bytes()` on the
/// same instance, plus a round-trip witness through the paired
/// [`From<String> for CaixaVersion`] constructor closing the two-way
/// `Self → Box<[u8]> → Vec<u8> → String → Self` cycle for the UTF-8-valid
/// bodies every `SemVer`-shaped `:versao` is by construction) and
/// [`tests::caixa_version_from_into_owned_box_bytes_and_vec_bytes_agree_on_every_shape`]
/// (cross-axis partition pin against the paired owned-input
/// [`From<CaixaVersion> for Vec<u8>`],
/// [`From<CaixaVersion> for Cow<'static, [u8]>`], and the pre-existing
/// borrowed [`AsRef<[u8]>`] byte-view impls on the same instance, closing
/// the "owned-input into `Vec<u8>` vs. `Cow<'static, [u8]>` vs. `Box<[u8]>`"
/// three-corner partition on the same wrapped body).
impl From<CaixaVersion> for Box<[u8]> {
    fn from(v: CaixaVersion) -> Box<[u8]> {
        v.0.into_bytes().into_boxed_slice()
    }
}

/// Trait-idiomatic *borrowed-input, [`Box<[u8]>`] output* byte-owned reverse
/// projection on the [`CaixaVersion`] newtype primitive — the borrowed-input
/// companion to the paired owned-input [`From<CaixaVersion> for Box<[u8]>`]
/// impl immediately above. Routes byte-for-byte through the substrate-
/// primitive [`CaixaVersion::as_str`] `pub const fn` accessor (via
/// [`str::as_bytes`] + [`Box::<[u8]>::from`]`(&[u8])`, which allocates a
/// fit-to-length boxed byte slice from the borrowed `&[u8]` in one heap
/// allocation without an intermediary [`Vec<u8>`]) so every consumer that
/// holds a borrowed [`&CaixaVersion`] and needs a [`Box<[u8]>`] — a
/// `[…].iter().map(Box::<[u8]>::from).collect::<Vec<_>>()` per-instance
/// materializer over `&[CaixaVersion]` (whose iterator yields
/// `&CaixaVersion`, not `CaixaVersion`, so the paired owned-input
/// [`From<CaixaVersion> for Box<[u8]>`] axis alone forces every call site
/// through an explicit `.clone()` / dereference restatement), a future
/// `HashMap::<Box<[u8]>, _>::from_iter` that keys off a borrowed-iteration
/// axis, a future generic `<T: for<'a> Into<Box<[u8]>>>`-bound byte-writer
/// on a per-caixa diagnostic column that walks the
/// `iter().map(Into::into)` shape verbatim — reaches the wrapped byte-
/// string through this one dispatch on the substrate primitive.
///
/// Second corner on the `{Self, &Self} → Box<[u8]>` byte-owned reverse-
/// projection family opened on the paired owned-input impl immediately
/// above. Rust's `From` trait does not derive the `From<&Self>` sibling
/// from a `From<Self>` impl (the blanket
/// `impl<T, U> From<&T> for U where T: Clone, U: From<T>` does not exist in
/// `core`), so every newtype that carries the owned-input byte-owned
/// reverse [`Box<[u8]>`] axis but not the borrowed-input axis forces every
/// borrowed call site through a `.clone()` /
/// `<Box<[u8]>>::from(v.clone())` detour whose type bounds have no
/// compile-time link back to the newtype.
///
/// Pinned load-bearing by
/// [`tests::caixa_version_from_borrowed_into_owned_box_bytes_routes_through_as_str_accessor`]
/// (byte-parity pin against [`CaixaVersion::as_str`] `.as_bytes()` via a
/// borrowed input, plus a source-survival witness against silent move-out)
/// and
/// [`tests::caixa_version_from_owned_and_borrowed_into_box_bytes_agree_on_every_shape`]
/// (cross-corner partition pin between owned-input move and borrowed-input
/// clone on the same wrapped body through the [`Box<[u8]>`] axis).
impl From<&CaixaVersion> for Box<[u8]> {
    fn from(v: &CaixaVersion) -> Box<[u8]> {
        Box::<[u8]>::from(v.as_str().as_bytes())
    }
}

/// Canonical Zig-style git-tag prefix every `feira publish` run writes
/// and every downstream consumer of a published caixa reads. A caixa
/// published at `:versao "0.1.0"` lands as a git tag `v0.1.0` on the
/// source repo's `origin` remote — the [`crate::CaixaVersion`] value
/// gates the version body, this constant gates the prefix the body
/// composes under.
///
/// Two production-code consumers carry this prefix on the same git
/// remote axis:
///
/// 1. [`caixa-feira`]'s `feira publish` verb (caixa-feira/src/cmd/publish.rs)
///    — the writer. Its `--prefix` clap flag defaults to this string
///    and the verb computes the tag as `format!("{prefix}{versao}")`
///    before `git tag -a <tag>` + `git push origin <tag>`.
/// 2. [`caixa-flux`]'s [`caixa-flux::cluster_bundle`] renderer
///    (caixa-flux/src/lib.rs) — the reader. Its
///    `ClusterBundleOpts::for_caixa` constructor defaults
///    `git_ref: GitRefSpec::Tag(...)` to `<prefix><versao>` so the
///    rendered `gitrepository.yaml` carries `ref: { tag: v<versao> }`
///    pointing `FluxCD`'s `GitRepository` reconciler at the exact tag
///    the publisher just wrote.
///
/// Until this lift landed both consumers carried the bare `"v"` byte
/// inline — `caixa-feira/src/cmd/publish.rs:22`'s clap
/// `default_value = "v"` and `caixa-flux/src/lib.rs:335`'s
/// `format!("v{}", caixa.versao)` literal. A future Zig-style-tag
/// convention rebrand (the substrate moving to plain `<versao>` tags
/// once the GitHub releases UI normalizes around the bare form, to
/// `release/<versao>` once a sibling forge convention adopts the
/// `<type>/<value>` slash-namespaced shape, or to a per-edition
/// override the operator pins through a future `:placement
/// :tag-prefix` slot) without a coordinated edit on both sides would
/// silently emit a `feira publish`-side tag at one shape (e.g.
/// `release/0.1.0`) and a `cluster_bundle`-side `ref: { tag: v0.1.0 }`
/// pointing at the prior shape — Flux's `GitRepository` reconciler
/// would loop forever looking for an upstream `v0.1.0` ref the publish
/// remote no longer carries, the dependent `HelmRelease`'s `chart:
/// sourceRef` would never resolve, every per-Servico apply would
/// silently come up with the prior reconciled state, and the failure
/// would surface at `kubectl describe gitrepository` time (the
/// `Status: Stalled` / `Reason: Failed` arm) far from the rebrand
/// commit's source.
///
/// Lifting the literal to one `&'static str` constant closes the drift
/// footgun structurally — both consumers read from the same memory,
/// so any future rebrand reaches both sites by construction and a CI
/// build that re-introduces a sibling inline `"v"` literal trips the
/// peer pinning tests
/// ([`caixa-feira`]'s `publish_prefix_default_pins_lifted_caixa_core_constant`,
/// [`caixa-flux`]'s `cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix`)
/// at the build-time fail-before-deploy posture every prior
/// load-bearing-string lift on this surface
/// ([`crate::DEFAULT_NAMESPACE`] a085b26,
/// [`crate::DEFAULT_LIBRARY_NAME`] 41438dc,
/// [`crate::DEFAULT_SERVICO_PORT`] 1e22add) establishes.
///
/// Authoring-side `:versao` gates already refuse the `"v"`-prefixed
/// publish tag shape leaking back into a version body — every typed
/// `:versao` surface (top-level `:versao`, `:upgrade-from :from`,
/// `:deps :versao`, `:deps-dev :versao`, `:membros :versao`,
/// `:children :versao`) routes through `semver::Version::parse` /
/// [`parse_requirement`], both of which reject the `v`-prefix as
/// invalid `SemVer`. The split — bare `SemVer` at the `:versao` slot,
/// `v<versao>` at the published git-tag axis — is the convention this
/// constant pins.
pub const DEFAULT_PUBLISH_TAG_PREFIX: &str = "v";

/// Canonical git remote name every `feira` writer-side verb pushes to —
/// the destination handle the operator-out-of-the-loop publish + deploy
/// chain (`feira publish`, `feira deploy --apply`, `feira app deploy
/// --apply`) names when it invokes `git push <remote> <ref>` against
/// the local clone of the source / k8s GitOps repo.
///
/// Three production-code consumers carry this remote name on the same
/// `git push` axis:
///
/// 1. [`caixa-feira`]'s `feira publish` verb (caixa-feira/src/cmd/publish.rs)
///    — the writer-side publish path. Its `--remote` clap flag defaults
///    to this string and the verb runs `git push <remote> <tag>` to push
///    the freshly written `v<versao>` tag upstream.
/// 2. [`caixa-feira`]'s `feira deploy --apply` verb
///    (caixa-feira/src/cmd/deploy.rs) — the writer-side Servico cluster-
///    deploy path. Its `push_origin` helper runs `git push origin HEAD`
///    against the k8s GitOps repo's working tree after upserting the
///    Servico's entry into the cluster's lareira-fleet-programs
///    HelmRelease values.
/// 3. [`caixa-feira`]'s `feira app deploy --apply` verb
///    (caixa-feira/src/cmd/app.rs) — the writer-side Aplicacao
///    cluster-deploy path. Its `push_origin` helper runs the same
///    `git push origin HEAD` against the k8s GitOps repo after writing
///    the rendered multi-doc YAML (programs.yaml entries + Cilium
///    NetworkPolicies + Gateway/HTTPRoute) to the cluster's tree.
///
/// Until this lift landed all three consumers carried the bare
/// `"origin"` byte inline — `publish.rs`'s clap `default_value = "origin"`,
/// `deploy.rs`'s `git(repo, ["push", "origin", "HEAD"])`, and
/// `app.rs`'s `git(repo, ["push", "origin", "HEAD"])`. A future
/// remote-naming-convention rebrand on any one side (the substrate
/// moving to `upstream` for forge-mirror clusters, to a per-tenant
/// remote naming convention once the operator-flux pipeline grows the
/// `:placement :remote` slot, or to the canonical multi-remote
/// `release` + `mirror` split every Erlang/OTP `release_handler` /
/// `relup` shop converges on once their git surface grows past one
/// upstream) without a coordinated edit on the other two would have
/// silently emitted a `git push` against a remote that doesn't exist
/// on the operator's clone (`fatal: '<remote>' does not appear to be
/// a git repository`) on one writer verb while the other two still
/// pushed to the old remote — operator-observed symptom: the publish
/// landed but the deploy didn't, or vice-versa, with the failure
/// surfacing as a partial-state rollout far from the rebrand commit's
/// source.
///
/// Lifting the literal to one `&'static str` constant closes the drift
/// footgun structurally — all three consumers read from the same
/// memory, so any future remote-naming rebrand reaches every writer
/// verb by construction and a CI build that re-introduces a sibling
/// inline `"origin"` literal trips the peer pinning tests
/// ([`caixa-feira`]'s `publish_remote_default_pins_lifted_caixa_core_constant`
/// on the clap-default axis, the sibling structural pins on the two
/// `push_origin` helpers) at the build-time fail-before-deploy
/// posture every prior load-bearing-string lift on this surface
/// ([`crate::DEFAULT_NAMESPACE`] a085b26, [`crate::DEFAULT_LIBRARY_NAME`]
/// 41438dc, [`crate::DEFAULT_SERVICO_PORT`] 1e22add,
/// [`crate::DEFAULT_PUBLISH_TAG_PREFIX`] 0a6a602,
/// [`crate::DEFAULT_FLUX_SYSTEM_NAMESPACE`] 7197d38) establishes.
///
/// Pairs with [`DEFAULT_PUBLISH_TAG_PREFIX`] on the same git remote
/// axis — `feira publish` runs `git push <DEFAULT_GIT_REMOTE>
/// <DEFAULT_PUBLISH_TAG_PREFIX><versao>` to push the typed `:versao`
/// body composed under the canonical prefix to the canonical remote.
/// Both halves of the publish-side convention now live in one place.
pub const DEFAULT_GIT_REMOTE: &str = "origin";

/// Canonical GitHub org name the pleme-io substrate defaults every un-
/// pinned caixa's source repo to — the org handle the two substrate-side
/// "no `:repositorio` / no `:fonte` declared, fall back to the canonical
/// org" paths compose their `github:<org>/<nome>` shorthand + full
/// `https://github.com/<org>/<nome>` URL under.
///
/// Two production-code consumers carry this org name on the same
/// canonical-substrate-default-git-org axis:
///
/// 1. [`caixa-feira`]'s `feira lock` verb's `resolve_stub` (caixa-feira/src/cmd/lock.rs)
///    — the resolver-side default. When a declared dep has no
///    `:fonte` block the stub resolver composes
///    `caixa_core::DepSource::default_github(<org>, &dep.nome)` to fill
///    the shorthand `github:<org>/<nome>` fallback the phase 1.B
///    `feira resolve` walker will resolve against upstream.
/// 2. [`caixa-flux`]'s [`caixa-flux::cluster_bundle`] renderer
///    (caixa-flux/src/lib.rs) — the renderer-side default. Its
///    `ClusterBundleOpts::for_caixa` constructor defaults
///    `git_url` to `format!("https://github.com/{org}/{}", caixa.nome)`
///    when the caixa carries no `:repositorio`, so the rendered
///    `gitrepository.yaml` points `FluxCD`'s `GitRepository`
///    reconciler at the substrate's canonical git host for un-pinned
///    caixas.
///
/// Until this lift landed both consumers carried the bare `"pleme-io"`
/// byte inline — `caixa-feira/src/cmd/lock.rs:61`'s
/// `default_github("pleme-io", …)` call and `caixa-flux/src/lib.rs`'s
/// `format!("https://github.com/pleme-io/{}", …)` literal. A future
/// substrate-side git-org migration (the pleme-io org renaming to a
/// short form, forking to a per-tenant `<org>-<tenant>` shape once the
/// operator-flux pipeline grows a `:placement :org` slot, or moving to
/// a self-hosted forge under a wholly-owned org name once the
/// substrate's forge-gen roadmap graduates past GitHub) without a
/// coordinated edit on both sides would silently emit a `feira lock`-
/// side `github:<old-org>/<nome>` fallback shorthand while the
/// `cluster_bundle`-side `gitrepository.yaml` pointed at the new org's
/// `<nome>` — the phase 1.B `feira resolve` walker would probe the
/// prior org's git host for a repo that migrated with the org, or vice-
/// versa: Flux's `GitRepository` reconciler would loop forever looking
/// for an upstream repo the old org handle no longer maps to, the
/// dependent `HelmRelease`'s `chart: sourceRef` would never resolve,
/// every per-Servico apply would silently come up with the prior
/// reconciled state, and the failure would surface at `kubectl describe
/// gitrepository` time (the `Status: Stalled` / `Reason: Failed` arm)
/// far from the org-migration commit's source.
///
/// Lifting the literal to one `&'static str` constant closes the drift
/// footgun structurally — both consumers read from the same memory, so
/// any future org migration reaches both sites by construction and a CI
/// build that re-introduces a sibling inline `"pleme-io"` literal trips
/// the peer pinning tests at the build-time fail-before-deploy posture
/// every prior load-bearing-string lift on this surface
/// ([`crate::DEFAULT_NAMESPACE`] a085b26,
/// [`crate::DEFAULT_LIBRARY_NAME`] 41438dc,
/// [`crate::DEFAULT_SERVICO_PORT`] 1e22add,
/// [`DEFAULT_PUBLISH_TAG_PREFIX`] 0a6a602,
/// [`DEFAULT_GIT_REMOTE`],
/// [`crate::DEFAULT_FLUX_SYSTEM_NAMESPACE`] 7197d38) establishes.
///
/// Distinct from the [`crate::PLEME_LABEL_PREFIX`] canonical pleme-io
/// label-namespace prefix (`"pleme.pleme.io"`, the K8s label-namespace
/// axis every substrate-emitted cluster object's `LABEL_APLICACAO` /
/// `LABEL_PROGRAM` / `LABEL_CONTRATO` axis shares) — these constants
/// sit on separate schema-contract surfaces (the git-host org handle
/// vs. the K8s label-namespace prefix) governed by independent rebrand
/// cycles, so a git-org rename must not couple the K8s label-namespace
/// axis to the git-host axis (or vice-versa). Splitting the two lets
/// each schema's future rebrand land independently at its canonical
/// const definition without silently coupling the surfaces — same
/// "byte-distinct, semantically distinct" discipline the
/// [`crate::PLEME_LABEL_PREFIX`] / [`crate::LABEL_APLICACAO`] /
/// [`crate::LABEL_PROGRAM`] / [`crate::LABEL_CONTRATO`] set establishes
/// on the peer per-K8s-label-namespace canonical-string surface.
pub const DEFAULT_PLEME_GIT_ORG: &str = "pleme-io";

/// Parse a dep's `:versao` string as a [`semver::VersionReq`].
///
/// Treats the literal `"*"` as "any version" (semver's wildcard).
pub fn parse_requirement(s: &str) -> Result<semver::VersionReq, VersionError> {
    if s == "*" {
        return Ok(semver::VersionReq::STAR);
    }
    semver::VersionReq::parse(s).map_err(|e| VersionError::requirement(s, e.to_string()))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VersionError {
    #[error("invalid version '{0}': {1}")]
    Semver(String, String),
    #[error("invalid version requirement '{0}': {1}")]
    Requirement(String, String),
}

// Fold the sole `VersionError::Semver(<into-String-expr>, <into-String-expr>)`
// wire-up site on [`CaixaVersion::parse`]'s [`semver::Version::parse`]
// `map_err` arm onto one substrate primitive — the paired
// `(String, String)` two-slot tuple-newtype [`VersionError::Semver`] on
// the [`CaixaVersion`] parser surface, the first of the two variants on
// the [`VersionError`] envelope's paired `(String, String)` tuple-newtype
// codec-magnitude family (its peer is [`VersionError::Requirement`] on
// the sibling [`parse_requirement`] surface). Same discipline the peer
// per-variant lifts on [`AplicacaoError`] / [`SupervisorError`] /
// [`UpgradeError`] / [`LayoutError`] / [`DepError`] / [`ManifestError`]
// / [`LimitsError`] / [`BehaviorError`] / [`DialetoError`] have
// converged through the "one substrate primitive per emit-site variant"
// ratchet: the sole wire-up site opens the identical
// `VersionError::Semver(<into-String-expr>, <into-String-expr>)` block
// against the parser-scoped `String` binding (`self.0.clone()`) and the
// derived `String` binding (`e.to_string()`) on the failing
// [`semver::Version::parse`] arm, so the fold routes the site through
// one dispatch on a uniform pair of `impl Into<String>` params,
// byte-equal to the pre-lift tuple-newtype construction on the same
// arguments. The `impl Into<String>` bound covers both the pre-lift
// `String` bindings and any future `&str` binding a downstream consumer
// might carry without forcing the caller to spell the `.into()`
// conversion at the wire-up site — the same shape the peer
// [`LimitsError::empty_byte_size`] / [`LimitsError::empty_duration`] /
// [`DialetoError::leitura`] folds carry on the single-slot `(String)`
// tuple-newtype cousins of the same tuple-newtype error-envelope family
// on the sibling parser surfaces. `#[must_use]` fires a compile warning
// at any wire-up that mistakenly discards the constructed error. The
// added [`PartialEq`] / [`Eq`] derives on the envelope (peer with the
// sibling [`LimitsError`] / [`DialetoError`] / [`DepError`] envelopes
// on the same axis) let the fail-before-pass-after byte-equality pins
// below trip a de-lift regression at caixa-core test time under
// `PartialEq` rather than at a downstream diagnostic shape drift.
//
// Every future consumer that wants to construct this variant outside
// [`CaixaVersion::parse`] (a deferred `feira lint --canonical-versao`
// per-caixa admission verb probing each authored top-level `:versao`
// value against the same [`semver::Version::parse`] gate, an M4 typed
// `mesh.pleme.io/v1alpha1/Servico` CR materializer's per-manifest
// admission validator re-checking one edited `:versao` slot against
// the [`CaixaVersion::parse`] semver floor, a per-`caixa.lisp` value-
// shape pre-emitter probing each declared `:versao` magnitude ahead of
// the operator's admit-cycle) now reaches the variant through one call
// rather than re-inlining the two-slot tuple-newtype block in lockstep.
impl VersionError {
    /// Construct a [`VersionError::Semver`] carrying the offending
    /// authoring string `value` and the underlying [`semver::Version::parse`]
    /// `reason` verbatim in the variant's two-slot tuple-newtype payload.
    /// Folds the uniform `Self::Semver(value.into(), reason.into())`
    /// tuple-newtype construction onto one substrate primitive so every
    /// wire-up on the variant reads through one dispatch rather than the
    /// pre-lift open-coded
    /// `VersionError::Semver(<into-String-expr>, <into-String-expr>)`
    /// block. The paired `impl Into<String>` bounds cover the pre-lift
    /// `String` wire-up shape on [`CaixaVersion::parse`]
    /// (`self.0.clone()` on the parser-scoped `String` field, `e.to_string()`
    /// on the derived `String` from the failing
    /// [`semver::Version::parse`] arm) without forcing the caller to
    /// spell the conversion at the wire-up site. Peer to the sibling
    /// [`VersionError::Requirement`] variant on the [`parse_requirement`]
    /// surface — the same `(String, String)` two-slot tuple-newtype axis
    /// of the paired [`VersionError`] envelope, but on the `SemVer`
    /// version-body parser surface rather than the version-requirement
    /// parser surface.
    #[must_use]
    pub fn semver(value: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Semver(value.into(), reason.into())
    }

    /// Construct a [`VersionError::Requirement`] carrying the offending
    /// authoring string `value` and the underlying
    /// [`semver::VersionReq::parse`] `reason` verbatim in the variant's
    /// two-slot tuple-newtype payload. Folds the uniform
    /// `Self::Requirement(value.into(), reason.into())` tuple-newtype
    /// construction onto one substrate primitive so every wire-up on the
    /// variant reads through one dispatch rather than the pre-lift open-
    /// coded `VersionError::Requirement(<into-String-expr>,
    /// <into-String-expr>)` block. Peer to the sibling
    /// [`VersionError::semver`] ctor on the [`CaixaVersion::parse`]
    /// surface — the same `(String, String)` two-slot tuple-newtype axis
    /// of the paired [`VersionError`] envelope, but on the version-
    /// requirement parser surface rather than the semver-version-body
    /// parser surface. Closes the last un-lifted variant on the
    /// [`VersionError`] envelope: every arm now reaches its emit site
    /// through one substrate-primitive dispatch, matching the "one
    /// substrate primitive per emit-site variant" ratchet the peer per-
    /// variant lifts on [`crate::AplicacaoError`] /
    /// [`crate::SupervisorError`] / [`crate::UpgradeError`] /
    /// [`crate::LayoutError`] / [`crate::DepError`] /
    /// [`crate::ManifestError`] / [`crate::LimitsError`] /
    /// [`crate::BehaviorError`] / [`crate::DialetoError`] have converged
    /// onto.
    #[must_use]
    pub fn requirement(value: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Requirement(value.into(), reason.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_round_trip() {
        let v: CaixaVersion = "1.2.3".into();
        assert_eq!(v.as_str(), "1.2.3");
        assert_eq!(v.parse().unwrap().to_string(), "1.2.3");
    }

    #[test]
    fn caixa_version_as_str_accessor_is_const_fn() {
        // Fail-before-pass-after pin on [`CaixaVersion::as_str`]'s
        // `const`-eval-surface posture. The accessor projects the typed
        // newtype's inner [`String`] through the `pub const fn`
        // [`String::as_str`] (const-stable since Rust 1.87, well within
        // the workspace MSRV) — any future accidental downgrade to
        // non-`const` fails `as_str_via_const_fn` at caixa-core build
        // time with E0015 (`cannot call non-const method`), strictly
        // stronger than a runtime `assert!`. Sibling of the peer
        // per-M2/M3/universal-axis `String → &str` scalar-accessor
        // family pins on the sibling `const`-eval-surface passes
        // ([`crate::Caixa::nome`] / [`crate::Caixa::versao`] at the
        // top-level manifest, [`crate::aplicacao::Membro::nome`] /
        // [`crate::aplicacao::Membro::versao_requirement`] at the M3
        // membership axis, [`crate::aplicacao::Entrada::hostname`] /
        // [`crate::aplicacao::Entrada::destination`] at the M3 ingress
        // axis, [`crate::supervisor::ChildSpec::nome`] /
        // [`crate::supervisor::ChildSpec::versao_requirement`] at the
        // M2 supervisor-tree axis,
        // [`crate::upgrade::UpgradeFromEntry::prior_versao`] at the M2
        // upgrade axis, [`crate::dep::Dep::nome`] /
        // [`crate::dep::Dep::versao_requirement`] at the dep-graph
        // axis, and the peer per-`:contratos` [`crate::aplicacao::WitContract::source`] /
        // [`crate::aplicacao::WitContract::destination`] /
        // [`crate::aplicacao::WitContract::world_ref`] trio the
        // sibling pin at 279823b already anchors).
        const fn as_str_via_const_fn(v: &CaixaVersion) -> &str {
            v.as_str()
        }
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            assert_eq!(as_str_via_const_fn(&v), v.as_str());
            assert_eq!(v.as_str(), versao);
        }
    }

    #[test]
    fn star_is_any() {
        let r = parse_requirement("*").unwrap();
        assert!(r.matches(&"0.1.0".parse().unwrap()));
        assert!(r.matches(&"99.0.0".parse().unwrap()));
    }

    #[test]
    fn caret_matches_minor_range() {
        let r = parse_requirement("^0.1").unwrap();
        assert!(r.matches(&"0.1.0".parse().unwrap()));
        assert!(r.matches(&"0.1.99".parse().unwrap()));
        assert!(!r.matches(&"0.2.0".parse().unwrap()));
    }

    #[test]
    fn invalid_version_errors() {
        let v: CaixaVersion = "not-a-version".into();
        assert!(v.parse().is_err());
    }

    #[test]
    fn semver_ctor_matches_tuple_literal_wrap_on_str_binding() {
        // Fail-before-pass-after byte-equality pin: the lifted
        // [`VersionError::semver`] inherent ctor projects a `&str`
        // binding pair through the paired `impl Into<String>` bounds
        // byte-equal to the pre-lift open-coded
        // `VersionError::Semver(<into-String-expr>, <into-String-expr>)`
        // tuple-literal on the same fixture, so any future silent
        // regression that swaps `.into()` for a divergent conversion
        // (a stray `String::from(str::trim(v))` normalization, a
        // parity-lossy `.to_lowercase()` fold, a `Cow<'_, str>` detour)
        // trips at caixa-core test time under `PartialEq` rather than
        // at a downstream diagnostic-shape drift on a consumer surface.
        // Same shape the peer
        // [`crate::LimitsError::empty_byte_size_ctor_matches_tuple_literal_wrap_on_str_binding`]
        // / [`crate::DialetoError::leitura_ctor_matches_tuple_literal_wrap_on_str_binding`]
        // pins carry on the sibling single-slot `(String)` tuple-newtype
        // cousins of the same tuple-newtype error-envelope family on the
        // sibling parser surfaces.
        let value: &str = "not-a-version";
        let reason: &str = "unexpected character 'n' while parsing major version number";
        assert_eq!(
            VersionError::semver(value, reason),
            VersionError::Semver(value.to_string(), reason.to_string()),
            "generated semver ctor over `&str` bindings must match \
             the pre-lift tuple-literal wrap on the same fixture",
        );
    }

    #[test]
    fn semver_ctor_matches_tuple_literal_wrap_on_string_binding() {
        // Fail-before-pass-after byte-equality pin on the paired owned-
        // `String` shape — the actual wire-up shape on
        // [`CaixaVersion::parse`] (`self.0.clone()` +
        // `e.to_string()`). Peer to the `&str` variant above; refuses
        // any future de-lift that inlines a divergent construction on
        // the owned-`String` path (a stray `.trim().to_string()`
        // normalization on either slot, a swap that routes the ctor
        // through the sibling [`VersionError::Requirement`] variant on
        // the paired parser surface).
        let value: String = String::from("1.2");
        let reason: String =
            String::from("unexpected end of input while parsing minor version number");
        assert_eq!(
            VersionError::semver(value.clone(), reason.clone()),
            VersionError::Semver(value, reason),
            "generated semver ctor over owned-`String` bindings must \
             match the pre-lift tuple-literal wrap on the same fixture",
        );
    }

    #[test]
    fn parse_semver_error_routes_through_semver_ctor() {
        // Fail-before-pass-after routes-through pin: refuses any future
        // de-lift of [`CaixaVersion::parse`]'s
        // [`semver::Version::parse`] `map_err` arm off the substrate
        // primitive. Sweeps three malformed authoring shapes (a bare
        // non-numeric, a partial `major.minor` shape, a stray leading
        // `v`-prefix that the [`DEFAULT_PUBLISH_TAG_PREFIX`] git-tag
        // convention rejects at the version-body slot) through the
        // parser and asserts the emitted [`VersionError`] equals the
        // ctor-built error verbatim under `PartialEq`, so any future
        // swap of the wire-up (an inline `Self::Semver(...)`
        // re-inlining, a routing detour through the sibling
        // [`VersionError::Requirement`] variant on the paired parser
        // surface, a swap of the ordering on the paired arguments)
        // trips at caixa-core test time rather than at a downstream
        // diagnostic drift on a `feira lint` / operator admission
        // callsite.
        for bad in ["not-a-version", "1.2", "v0.1.0"] {
            let v: CaixaVersion = bad.into();
            let err = v
                .parse()
                .expect_err("malformed versao fixture must fail semver parsing");
            let semver_reason = match semver::Version::parse(bad) {
                Err(e) => e.to_string(),
                Ok(_) => unreachable!(
                    "fixture `{bad}` is documented as a `SemVer` \
                     rejection but parsed cleanly — the pin's oracle \
                     drifted from `semver`'s current shape",
                ),
            };
            assert_eq!(
                err,
                VersionError::semver(bad, semver_reason),
                "CaixaVersion::parse must route its semver `map_err` \
                 arm through the lifted VersionError::semver ctor on \
                 the same offending value and semver reason",
            );
        }
    }

    #[test]
    fn default_git_remote_pins_canonical_origin_byte() {
        // Bridge-arm pin: [`DEFAULT_GIT_REMOTE`] resolves to the
        // canonical `"origin"` byte today, the same remote-handle every
        // `git clone <url>` invocation populates by default and every
        // peer `feira` writer-side verb (`feira publish`, `feira deploy
        // --apply`, `feira app deploy --apply`) names when it invokes
        // `git push <remote> <ref>` against the local clone. Pin the
        // literal here (peer with the
        // [`DEFAULT_PUBLISH_TAG_PREFIX`] / [`crate::DEFAULT_SERVICO_PORT`]
        // / [`crate::DEFAULT_NAMESPACE`] / [`crate::DEFAULT_LIBRARY_NAME`]
        // / [`crate::DEFAULT_FLUX_SYSTEM_NAMESPACE`] canonical-literal
        // pins on the sibling lifted-constant surfaces) so a future
        // remote-naming rebrand surfaces here as a coordinated edit-
        // point: the sibling [`caixa-feira`]
        // `publish_remote_default_pins_lifted_caixa_core_constant`
        // pinning test already pins the equality at the clap-default
        // axis; this pin closes the second coordinate of the
        // triangle by anchoring the lifted constant's current byte
        // to the canonical git-default-remote convention's documented
        // shape.
        assert_eq!(DEFAULT_GIT_REMOTE, "origin");
    }

    #[test]
    fn default_pleme_git_org_pins_canonical_pleme_io_byte() {
        // Bridge-arm pin: [`DEFAULT_PLEME_GIT_ORG`] resolves to the
        // canonical `"pleme-io"` GitHub-org-handle today, the same org
        // name every peer substrate-side default-git-source consumer
        // ([`caixa-feira`]'s `feira lock` `resolve_stub` for the
        // per-dep `:fonte`-elided `github:<org>/<nome>` fallback,
        // [`caixa-flux`]'s `ClusterBundleOpts::for_caixa` constructor
        // for the per-caixa `:repositorio`-elided
        // `https://github.com/<org>/<nome>` fallback) fills into its
        // per-consumer render/resolve compose site. Pin the literal
        // here (peer with the [`DEFAULT_PUBLISH_TAG_PREFIX`] /
        // [`DEFAULT_GIT_REMOTE`] canonical-literal pins on the sibling
        // lifted-constant surfaces) so a future substrate-side git-org
        // migration surfaces here as a coordinated edit-point: both
        // sibling consumer sites already thread through the same
        // `&'static str`, this pin anchors the lifted constant's
        // current byte to the canonical substrate-git-org convention's
        // documented shape.
        assert_eq!(DEFAULT_PLEME_GIT_ORG, "pleme-io");
    }

    #[test]
    fn requirement_ctor_matches_tuple_literal_wrap_on_str_binding() {
        // Fail-before-pass-after byte-equality pin: the lifted
        // [`VersionError::requirement`] inherent ctor projects a `&str`
        // binding pair through the paired `impl Into<String>` bounds
        // byte-equal to the pre-lift open-coded
        // `VersionError::Requirement(<into-String-expr>, <into-String-expr>)`
        // tuple-literal on the same fixture. Same shape the peer
        // [`VersionError::semver_ctor_matches_tuple_literal_wrap_on_str_binding`]
        // pin carries on the sibling [`VersionError::Semver`] variant of
        // the same `(String, String)` two-slot tuple-newtype envelope.
        let value: &str = "not-a-req";
        let reason: &str = "unexpected character 'n' while parsing major version number";
        assert_eq!(
            VersionError::requirement(value, reason),
            VersionError::Requirement(value.to_string(), reason.to_string()),
            "generated requirement ctor over `&str` bindings must match \
             the pre-lift tuple-literal wrap on the same fixture",
        );
    }

    #[test]
    fn requirement_ctor_matches_tuple_literal_wrap_on_string_binding() {
        // Fail-before-pass-after byte-equality pin on the paired owned-
        // `String` shape. Peer to the `&str` variant above; refuses any
        // future de-lift that inlines a divergent construction on the
        // owned-`String` path (a stray `.trim().to_string()` normalization
        // on either slot, a swap that routes the ctor through the sibling
        // [`VersionError::Semver`] variant on the paired parser surface,
        // an argument-ordering swap on the paired slots).
        let value: String = String::from("^bogus");
        let reason: String = String::from("unexpected character while parsing requirement");
        assert_eq!(
            VersionError::requirement(value.clone(), reason.clone()),
            VersionError::Requirement(value, reason),
            "generated requirement ctor over owned-`String` bindings must \
             match the pre-lift tuple-literal wrap on the same fixture",
        );
    }

    #[test]
    fn parse_requirement_error_routes_through_requirement_ctor() {
        // Fail-before-pass-after routes-through pin: refuses any future
        // de-lift of [`parse_requirement`]'s
        // [`semver::VersionReq::parse`] `map_err` arm off the substrate
        // primitive. Sweeps three malformed authoring shapes (a bare
        // non-numeric, a stray operator with no version body, a
        // caret-prefixed non-numeric that the [`semver::VersionReq`]
        // grammar rejects at the operator-body slot) through the parser
        // and asserts the emitted [`VersionError`] equals the ctor-built
        // error verbatim under `PartialEq`, so any future swap of the
        // wire-up (an inline `Self::Requirement(...)` re-inlining, a
        // routing detour through the sibling [`VersionError::Semver`]
        // variant on the paired parser surface, an argument-ordering
        // swap on the paired slots) trips at caixa-core test time rather
        // than at a downstream diagnostic drift on a `feira lock` /
        // resolver admission callsite. The `"*"` wildcard short-circuit
        // is deliberately excluded from the sweep — it returns
        // [`semver::VersionReq::STAR`] before reaching the parser arm.
        for bad in ["not-a-req", "^", "^bogus"] {
            let err = parse_requirement(bad)
                .expect_err("malformed requirement fixture must fail parsing");
            let semver_reason = match semver::VersionReq::parse(bad) {
                Err(e) => e.to_string(),
                Ok(_) => unreachable!(
                    "fixture `{bad}` is documented as a `VersionReq` \
                     rejection but parsed cleanly — the pin's oracle \
                     drifted from `semver`'s current shape",
                ),
            };
            assert_eq!(
                err,
                VersionError::requirement(bad, semver_reason),
                "parse_requirement must route its `map_err` arm through \
                 the lifted VersionError::requirement ctor on the same \
                 offending value and semver reason",
            );
        }
    }

    #[test]
    fn default_publish_tag_prefix_pins_canonical_v_byte() {
        // Bridge-arm pin: [`DEFAULT_PUBLISH_TAG_PREFIX`] resolves to the
        // canonical Zig-style `"v"` byte today, the same prefix every
        // peer doc-comment on the typed `:versao` surfaces (the
        // top-level `:versao` `validate_versao` cascade at
        // caixa-core/src/manifest.rs:646, the four sibling per-axis
        // `:versao` requirement gates that name the publish-side
        // `v<versao>` tag inline in their bodies) cites as the
        // canonical convention. Pin the literal here (peer with the
        // [`crate::DEFAULT_SERVICO_PORT`] / [`crate::DEFAULT_NAMESPACE`]
        // / [`crate::DEFAULT_LIBRARY_NAME`] canonical-literal pins on
        // the sibling lifted-constant surfaces) so a future rebrand of
        // the constant surfaces here as a coordinated edit-point: both
        // sibling pinning tests on the two consumer crates
        // ([`caixa-feira`] `publish_prefix_default_pins_lifted_caixa_core_constant`,
        // [`caixa-flux`] `cluster_bundle_default_git_tag_uses_lifted_caixa_core_prefix`)
        // already pin the equality at the consumer-default axis; this
        // pin closes the third coordinate of the triangle by anchoring
        // the lifted constant's current byte to the canonical Zig-style
        // convention's documented shape.
        assert_eq!(DEFAULT_PUBLISH_TAG_PREFIX, "v");
    }

    #[test]
    fn caixa_version_as_ref_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl AsRef<str> for CaixaVersion` — asserts the standard-
        // library trait impl and the substrate-primitive
        // [`CaixaVersion::as_str`] `pub const fn` accessor resolve to
        // the same `&str` per instance, so any future silent detour
        // that routes the impl through a divergent projection (a
        // `Cow<'_, str>` intermediate, a stray `.to_lowercase()`
        // normalization, a swap onto a per-arm inline `&self.0.as_str()`
        // re-inlining, a swap onto a divergent [`String::trim`]
        // fold) trips at caixa-core test time under `PartialEq`
        // rather than at a downstream `impl AsRef<str>`-bound
        // consumer's silent split. Sweeps four authoring shapes (a
        // canonical release version, a pre-release build-metadata
        // version, the zero-version canonical unset baseline, and
        // the empty-string byte the caller-side default-construct
        // path composes) so every non-degenerate arm of the wrapped
        // `String` storage is covered. Peer of the sibling
        // [`caixa_version_as_str_accessor_is_const_fn`] const-eval
        // pin on the same [`CaixaVersion::as_str`] primitive — the
        // two pins together cover the const-eval axis (the pin above)
        // and the trait-projection axis (this pin) of the same
        // substrate-primitive scalar accessor.
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            assert_eq!(
                <CaixaVersion as AsRef<str>>::as_ref(&v),
                v.as_str(),
                "AsRef<str> impl must byte-equal CaixaVersion::as_str \
                 on the same instance — divergence signals a silent \
                 detour off the substrate-primitive accessor",
            );
            assert_eq!(
                <CaixaVersion as AsRef<str>>::as_ref(&v),
                versao,
                "AsRef<str> impl must byte-equal the pre-lift wrapped \
                 String storage on round-trip through the From<&str> \
                 constructor — divergence signals a normalization \
                 detour on either the constructor or the accessor",
            );
        }
    }

    #[test]
    fn caixa_version_as_ref_str_routes_through_display_via_shared_accessor() {
        // Fail-before-pass-after byte-parity pin on the three-path
        // convergence discipline the substrate primitive now carries
        // on the `&str`-projection axis: `<CaixaVersion as
        // AsRef<str>>::as_ref(&v)` (the newly lifted impl),
        // `format!("{v}")` (the pre-existing [`fmt::Display`] impl),
        // and `v.as_str()` (the substrate-primitive `pub const fn`
        // accessor both trait impls delegate through) must resolve to
        // the same byte-string on every instance. Refuses any future
        // divergence between the two trait impls (a stray
        // [`fmt::Display::fmt`] rewrite that inlines
        // `f.write_str(&self.0)` on the wrapped `String` directly,
        // bypassing the shared accessor; a hypothetical `AsRef<str>`
        // rewrite that inlines the same `&self.0` field-access) that
        // would silently split the two projection paths of the same
        // typed newtype. Mirrors the sibling three-path-convergence
        // discipline the peer [`RestartStrategy`] typed enum carries
        // on its `Display` / `as_str` / `Serialize` triple (aplicacao.rs
        // pin `restart_strategy_display_matches_serialized_wire_byte_string`).
        for versao in ["0.1.0", "1.2.3-alpha.1", ""] {
            let v: CaixaVersion = versao.into();
            let via_as_ref: &str = <CaixaVersion as AsRef<str>>::as_ref(&v);
            let via_display: String = format!("{v}");
            let via_accessor: &str = v.as_str();
            assert_eq!(via_as_ref, via_accessor);
            assert_eq!(via_display, via_accessor);
            assert_eq!(via_as_ref, via_display.as_str());
        }
    }

    #[test]
    fn caixa_version_borrow_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl std::borrow::Borrow<str> for CaixaVersion` — asserts the
        // standard-library trait impl and the substrate-primitive
        // [`CaixaVersion::as_str`] `pub const fn` accessor resolve to
        // the same `&str` per instance, so any future silent detour
        // that routes the impl through a divergent projection (a
        // `Cow<'_, str>` intermediate, a stray `.to_lowercase()`
        // normalization, a swap onto a per-arm inline `&self.0.as_str()`
        // re-inlining that bypasses the shared accessor) trips at
        // caixa-core test time under `PartialEq` rather than at a
        // downstream `Borrow<str>`-bound collection API's silent
        // hash-mismatch on the load-bearing HashMap-key axis. Peer of
        // the sibling
        // [`caixa_version_as_ref_str_routes_through_as_str_accessor`]
        // byte-parity pin on the paired [`AsRef<str>`] impl — both cover
        // the borrow-projection axis of the same substrate primitive.
        use std::borrow::Borrow;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            assert_eq!(
                <CaixaVersion as Borrow<str>>::borrow(&v),
                v.as_str(),
                "Borrow<str> impl must byte-equal CaixaVersion::as_str \
                 on the same instance — divergence signals a silent \
                 detour off the substrate-primitive accessor",
            );
            assert_eq!(
                <CaixaVersion as Borrow<str>>::borrow(&v),
                versao,
                "Borrow<str> impl must byte-equal the pre-lift wrapped \
                 String storage on round-trip through the From<&str> \
                 constructor",
            );
        }
    }

    #[test]
    fn caixa_version_borrow_str_and_as_ref_str_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin on the two
        // trait impls on the same borrow-projection axis: the lifted
        // [`std::borrow::Borrow<str>`] impl (this commit) and the paired
        // [`AsRef<str>`] impl (a086 lift) must resolve to the same `&str`
        // per instance, both routing through the shared substrate-
        // primitive [`CaixaVersion::as_str`] accessor. Refuses any future
        // silent split between the two trait impls (a stray
        // [`AsRef::as_ref`] rewrite that inlines `&self.0.as_str()` on
        // the wrapped [`String`] directly, bypassing the shared
        // accessor; a hypothetical [`Borrow::borrow`] rewrite that
        // inlines the same `&self.0` field-access) that would silently
        // split the two projection paths of the same typed newtype and
        // break the [`std::borrow::Borrow`] safety contract's
        // "hash-agrees on the borrowed view" invariant the collection
        // APIs rely on. Mirrors the sibling three-path convergence
        // discipline the peer
        // [`caixa_version_as_ref_str_routes_through_display_via_shared_accessor`]
        // pin carries on the `AsRef<str>` / `Display` / `as_str` triple.
        use std::borrow::Borrow;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrow: &str = <CaixaVersion as Borrow<str>>::borrow(&v);
            let via_as_ref: &str = <CaixaVersion as AsRef<str>>::as_ref(&v);
            let via_accessor: &str = v.as_str();
            assert_eq!(via_borrow, via_accessor);
            assert_eq!(via_as_ref, via_accessor);
            assert_eq!(via_borrow, via_as_ref);
        }
    }

    #[test]
    fn caixa_version_borrow_str_enables_hashmap_lookup_by_borrowed_key() {
        // Fail-before-pass-after contract-witness pin on the
        // [`std::borrow::Borrow<str>`] safety contract: a
        // [`std::collections::HashMap`] keyed by owned [`CaixaVersion`]
        // must resolve `.get::<str>("<versao>")` probes through the
        // borrowed `&str` view of a stored key to the same slot, and
        // (`String::hash` calls `str::hash` on bytes, and the
        // [`CaixaVersion`] derived [`Hash`] impl hashes the wrapped
        // [`String`] field) the borrowed and owned hash must agree on
        // every fixture. Refuses any future silent regression that would
        // break the hash-agrees invariant (a
        // [`Hash for CaixaVersion`] hand-written impl that diverges from
        // the derived shape, a [`Borrow<str>::borrow`] rewrite that
        // routes through a normalization detour, an `Eq` hand-written
        // impl that diverges from field-wise equality) —
        // [`HashMap::get<Q>`] would return [`None`] on a key that
        // structurally lives in the map, which is the exact silent
        // failure the [`std::borrow::Borrow`] documented safety contract
        // rules out. The load-bearing use-case this impl was added for:
        // per-`:versao` collection APIs must be probed by borrowed
        // `&str` without a per-probe [`CaixaVersion::from(&str)`]
        // allocation.
        use std::collections::HashMap;
        let mut map: HashMap<CaixaVersion, u32> = HashMap::new();
        for (i, versao) in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""].iter().enumerate() {
            let key: CaixaVersion = (*versao).into();
            map.insert(key, u32::try_from(i).unwrap());
        }
        for (i, versao) in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""].iter().enumerate() {
            let hit = map.get(*versao).unwrap_or_else(|| {
                panic!(
                    "HashMap<CaixaVersion, _>::get(&str) must reach the \
                     slot inserted under CaixaVersion::from({versao:?}) \
                     through the Borrow<str> bound — a miss signals the \
                     borrowed-vs-owned hash-agrees invariant broke",
                )
            });
            assert_eq!(*hit, u32::try_from(i).unwrap());
        }
        assert!(
            !map.contains_key("does-not-exist"),
            "HashMap<CaixaVersion, _>::contains_key(&str) on an absent \
             key must return false, not accidentally hash-collide onto \
             a stored slot — the miss path must respect the same \
             invariant as the hit path",
        );
    }

    #[test]
    fn caixa_version_from_into_owned_string_returns_wrapped_body() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<CaixaVersion> for String` — asserts the owned-input
        // reverse-projection routes the wrapper's own heap allocation
        // through verbatim (no re-copy, no normalization detour) so
        // `String::from(v)` returns the same bytes `v.as_str()`
        // borrows. Refuses any future silent detour that would swap
        // the move on `v.0` for an allocating `.as_str().to_owned()` /
        // `.to_string()` cascade (the pre-lift compose shape), a stray
        // `.trim().to_owned()` normalization, or a routing through the
        // sibling [`fmt::Display`] emitter that would introduce a
        // formatter round-trip.
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let expected = v.as_str().to_owned();
            let owned: String = String::from(v);
            assert_eq!(
                owned, expected,
                "String::from(v) must return the wrapper's own bytes verbatim",
            );
            assert_eq!(
                owned, versao,
                "String::from(v) must round-trip byte-equal through the From<&str> constructor",
            );
        }
    }

    #[test]
    fn caixa_version_from_into_owned_string_and_as_str_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the owned-
        // input [`From<CaixaVersion> for String`] reverse projection
        // and the borrowed [`AsRef<str>`] projection resolve to the
        // same bytes on every instance, and the paired forward
        // [`From<String> for CaixaVersion`] constructor closes the
        // `Self → String → Self` round-trip by construction. Refuses
        // any future silent split between the owned-move reverse axis
        // and the borrowed-clone AsRef axis (a stray normalization on
        // one path only) that would let `String::from(v)` and
        // `v.as_ref::<str>()` diverge on the same instance.
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_as_ref: String = <CaixaVersion as AsRef<str>>::as_ref(&v).to_owned();
            let via_to_string: String = v.to_string();
            let via_from: String = String::from(v.clone());
            assert_eq!(via_from, via_as_ref);
            assert_eq!(via_from, via_to_string);
            let round_trip: CaixaVersion = via_from.clone().into();
            assert_eq!(round_trip, v);
        }
    }

    #[test]
    fn caixa_version_from_borrowed_into_owned_string_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<&CaixaVersion> for String` — asserts the
        // borrowed-input reverse projection allocates a fresh
        // [`String`] whose bytes byte-equal the substrate-primitive
        // [`CaixaVersion::as_str`] accessor on the same instance,
        // preserving the source [`CaixaVersion`] intact (no move-out).
        // Refuses any future silent detour that would route the impl
        // through a divergent projection (a stray normalization step,
        // a swap onto the sibling [`fmt::Display`]-routed
        // [`ToString::to_string`] surface, a re-inlining that
        // dereferences `&self.0` outside the shared accessor).
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: String = String::from(&v);
            assert_eq!(
                via_borrowed,
                v.as_str(),
                "String::from(&v) must byte-equal CaixaVersion::as_str",
            );
            // The borrowed-input impl must not move out of the source.
            assert_eq!(
                v.as_str(),
                versao,
                "source CaixaVersion must survive borrowed-input projection"
            );
        }
    }

    #[test]
    fn caixa_version_from_owned_and_borrowed_into_string_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the paired
        // owned-input [`From<CaixaVersion> for String`] and
        // borrowed-input [`From<&CaixaVersion> for String`] impls
        // resolve to the same bytes on every instance, closing the
        // "owned-input move vs. borrowed-input clone" bifurcation on
        // the same wrapped body. Refuses any future silent split
        // between the two corners (a normalization on one path only, a
        // divergent routing that would let `String::from(v.clone())`
        // and `String::from(&v)` disagree on the same body).
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: String = String::from(&v);
            let via_owned: String = String::from(v.clone());
            assert_eq!(via_owned, via_borrowed);
            assert_eq!(via_borrowed, versao);
        }
    }

    #[test]
    fn caixa_version_from_into_owned_cow_str_returns_owned_wrapped_body() {
        // Fail-before-pass-after byte-parity + [`Cow::Owned`]-arm pin
        // on the lifted `impl From<CaixaVersion> for
        // std::borrow::Cow<'static, str>` — asserts the owned-input
        // reverse projection routes the wrapper's own heap allocation
        // through `Cow::Owned(v.0)` verbatim (no re-copy, no
        // normalization detour, no `Cow::Borrowed` misclassification
        // that would demand a `&'static str` the runtime wrapper cannot
        // carry), so the emitted [`Cow`] byte-equals the substrate-
        // primitive [`CaixaVersion::as_str`] accessor on the same
        // instance and round-trips byte-equal through the paired
        // forward [`From<String> for CaixaVersion`] constructor.
        // Refuses any future silent detour: a swap of the move on
        // `v.0` for an allocating `.as_str().to_owned()` cascade (the
        // pre-lift compose shape would double-allocate a fresh
        // intermediary [`String`] on the way to the same
        // [`Cow::Owned`] arm), a stray `.trim().to_owned()`
        // normalization, or a mis-routing through
        // [`Cow::Borrowed`] on a non-`'static` byte-string that would
        // not type-check.
        use std::borrow::Cow;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let expected = v.as_str().to_owned();
            let cow: Cow<'static, str> = Cow::from(v.clone());
            assert!(
                matches!(cow, Cow::Owned(_)),
                "From<CaixaVersion> for Cow<'static, str> must land on \
                 the Cow::Owned arm — a runtime String wrapper cannot \
                 promise the 'static lifetime the Cow::Borrowed arm \
                 requires",
            );
            assert_eq!(
                cow.as_ref(),
                expected,
                "Cow::from(v) must return the wrapper's own bytes verbatim",
            );
            let round_trip: CaixaVersion = cow.into_owned().into();
            assert_eq!(
                round_trip, v,
                "Cow::from(v).into_owned() must round-trip byte-equal \
                 through the From<String> constructor",
            );
        }
    }

    #[test]
    fn caixa_version_from_into_owned_cow_str_and_string_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the owned-
        // input [`From<CaixaVersion> for Cow<'static, str>`] reverse
        // projection and the paired owned-input
        // [`From<CaixaVersion> for String`] reverse projection resolve
        // to the same bytes on every instance, and both agree with the
        // borrowed [`AsRef<str>`] surface on the same wrapped body.
        // Refuses any future silent split between the two owned-input
        // reverse-projection axes (a stray normalization on one path
        // only, a divergent routing that would let
        // `Cow::from(v.clone())` and `String::from(v.clone())` disagree
        // on the same body) that would silently split the same-shape
        // owned-move discipline across the two reverse-projection
        // targets.
        use std::borrow::Cow;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_string: String = String::from(v.clone());
            let via_cow: Cow<'static, str> = Cow::from(v.clone());
            let via_as_ref: &str = <CaixaVersion as AsRef<str>>::as_ref(&v);
            assert_eq!(via_cow.as_ref(), via_string.as_str());
            assert_eq!(via_cow.as_ref(), via_as_ref);
            assert_eq!(via_cow.as_ref(), versao);
        }
    }

    #[test]
    fn caixa_version_from_borrowed_into_owned_cow_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity + [`Cow::Owned`]-arm pin
        // on the lifted `impl From<&CaixaVersion> for
        // std::borrow::Cow<'static, str>` — asserts the borrowed-input
        // reverse projection allocates a fresh [`Cow::Owned`] whose
        // bytes byte-equal the substrate-primitive
        // [`CaixaVersion::as_str`] accessor on the same instance,
        // preserving the source [`CaixaVersion`] intact (no move-out).
        // Refuses any future silent detour that would route the impl
        // through a divergent projection (a stray normalization step,
        // a mis-routing onto [`Cow::Borrowed`] on a non-`'static`
        // byte-string that would not type-check, a re-inlining that
        // dereferences `&self.0` outside the shared accessor).
        use std::borrow::Cow;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Cow<'static, str> = Cow::from(&v);
            assert!(
                matches!(via_borrowed, Cow::Owned(_)),
                "From<&CaixaVersion> for Cow<'static, str> must land on \
                 the Cow::Owned arm — a runtime String wrapper cannot \
                 promise the 'static lifetime the Cow::Borrowed arm \
                 requires",
            );
            assert_eq!(
                via_borrowed.as_ref(),
                v.as_str(),
                "Cow::from(&v) must byte-equal CaixaVersion::as_str",
            );
            // The borrowed-input impl must not move out of the source.
            assert_eq!(
                v.as_str(),
                versao,
                "source CaixaVersion must survive borrowed-input projection",
            );
        }
    }

    #[test]
    fn caixa_version_from_owned_and_borrowed_into_cow_str_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the paired
        // owned-input [`From<CaixaVersion> for Cow<'static, str>`] and
        // borrowed-input [`From<&CaixaVersion> for Cow<'static, str>`]
        // impls resolve to the same bytes on every instance, closing
        // the "owned-input move vs. borrowed-input clone" bifurcation
        // on the same wrapped body through the [`Cow<'static, str>`]
        // axis. Refuses any future silent split between the two
        // corners (a normalization on one path only, a divergent
        // routing that would let `Cow::from(v.clone())` and
        // `Cow::from(&v)` disagree on the same body). Both corners
        // must land on [`Cow::Owned`] — the runtime wrapper's storage
        // rules out the borrowed arm on both input shapes alike.
        use std::borrow::Cow;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Cow<'static, str> = Cow::from(&v);
            let via_owned: Cow<'static, str> = Cow::from(v.clone());
            assert!(matches!(via_borrowed, Cow::Owned(_)));
            assert!(matches!(via_owned, Cow::Owned(_)));
            assert_eq!(via_owned.as_ref(), via_borrowed.as_ref());
            assert_eq!(via_borrowed.as_ref(), versao);
        }
    }

    #[test]
    fn caixa_version_from_into_owned_box_str_returns_wrapped_body() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<CaixaVersion> for Box<str>` — asserts the owned-
        // input reverse projection routes the wrapper's own heap
        // allocation through [`String::into_boxed_str`] verbatim (no
        // re-copy of the underlying bytes on the fixed-capacity path;
        // `String::into_boxed_str` reuses the same `Vec<u8>` buffer
        // when length matches capacity), so `Box::<str>::from(v)`
        // returns the same bytes `v.as_str()` borrows and round-trips
        // byte-equal through the paired forward
        // [`From<String> for CaixaVersion`] constructor closing the
        // two-way `Self → Box<str> → Self` cycle by construction.
        // Refuses any future silent detour that would swap
        // `v.0.into_boxed_str()` for an allocating
        // `.as_str().to_owned().into_boxed_str()` cascade (the pre-lift
        // compose shape would double-allocate a fresh intermediary
        // [`String`] on the way to the same [`Box<str>`] slot), a
        // stray `.trim().to_owned().into_boxed_str()` normalization,
        // or a routing through the sibling [`fmt::Display`] emitter
        // that would introduce a formatter round-trip.
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let expected = v.as_str().to_owned();
            let boxed: Box<str> = Box::<str>::from(v.clone());
            assert_eq!(
                boxed.as_ref(),
                expected.as_str(),
                "Box::<str>::from(v) must return the wrapper's own bytes verbatim",
            );
            let round_trip: CaixaVersion = boxed.into_string().into();
            assert_eq!(
                round_trip, v,
                "Box::<str>::from(v).into_string() must round-trip byte-equal \
                 through the From<String> constructor",
            );
        }
    }

    #[test]
    fn caixa_version_from_into_owned_box_str_and_string_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the owned-
        // input [`From<CaixaVersion> for Box<str>`] reverse projection
        // and the paired owned-input [`From<CaixaVersion> for String`]
        // and [`From<CaixaVersion> for Cow<'static, str>`] reverse
        // projections resolve to the same bytes on every instance, and
        // all three agree with the borrowed [`AsRef<str>`] surface on
        // the same wrapped body. Refuses any future silent split
        // between the three owned-input reverse-projection axes (a
        // stray normalization on one path only, a divergent routing
        // that would let `Box::<str>::from(v.clone())`,
        // `String::from(v.clone())`, and `Cow::from(v.clone())`
        // disagree on the same body) that would silently split the
        // same-shape owned-move discipline across the three
        // reverse-projection targets.
        use std::borrow::Cow;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_string: String = String::from(v.clone());
            let via_cow: Cow<'static, str> = Cow::from(v.clone());
            let via_box: Box<str> = Box::<str>::from(v.clone());
            let via_as_ref: &str = <CaixaVersion as AsRef<str>>::as_ref(&v);
            assert_eq!(via_box.as_ref(), via_string.as_str());
            assert_eq!(via_box.as_ref(), via_cow.as_ref());
            assert_eq!(via_box.as_ref(), via_as_ref);
            assert_eq!(via_box.as_ref(), versao);
        }
    }

    #[test]
    fn caixa_version_from_borrowed_into_owned_box_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<&CaixaVersion> for Box<str>` — asserts the
        // borrowed-input reverse projection allocates a fresh
        // [`Box<str>`] whose bytes byte-equal the substrate-primitive
        // [`CaixaVersion::as_str`] accessor on the same instance,
        // preserving the source [`CaixaVersion`] intact (no move-out).
        // Refuses any future silent detour that would route the impl
        // through a divergent projection (a stray normalization step,
        // a swap onto the sibling [`fmt::Display`]-routed
        // [`ToString::to_string`] surface followed by
        // `.into_boxed_str()`, a re-inlining that dereferences
        // `&self.0` outside the shared accessor).
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Box<str> = Box::<str>::from(&v);
            assert_eq!(
                via_borrowed.as_ref(),
                v.as_str(),
                "Box::<str>::from(&v) must byte-equal CaixaVersion::as_str",
            );
            // The borrowed-input impl must not move out of the source.
            assert_eq!(
                v.as_str(),
                versao,
                "source CaixaVersion must survive borrowed-input projection",
            );
        }
    }

    #[test]
    fn caixa_version_from_owned_and_borrowed_into_box_str_agree_on_every_shape() {
        // Fail-before-pass-after cross-corner partition pin: the paired
        // owned-input [`From<CaixaVersion> for Box<str>`] and
        // borrowed-input [`From<&CaixaVersion> for Box<str>`] impls
        // resolve to the same bytes on every instance, closing the
        // "owned-input move vs. borrowed-input clone" bifurcation on
        // the same wrapped body through the [`Box<str>`] axis. Refuses
        // any future silent split between the two corners (a
        // normalization on one path only, a divergent routing that
        // would let `Box::<str>::from(v.clone())` and
        // `Box::<str>::from(&v)` disagree on the same body).
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Box<str> = Box::<str>::from(&v);
            let via_owned: Box<str> = Box::<str>::from(v.clone());
            assert_eq!(via_owned.as_ref(), via_borrowed.as_ref());
            assert_eq!(via_borrowed.as_ref(), versao);
        }
    }

    #[test]
    fn caixa_version_from_into_owned_arc_str_returns_wrapped_body() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<CaixaVersion> for std::sync::Arc<str>` — asserts
        // the owned-input reverse projection routes the wrapper's own
        // [`String`] body through [`std::sync::Arc::<str>::from`]
        // verbatim (one heap allocation of the atomically-refcounted
        // slab, no intermediary [`String`] or [`Box<str>`] on the
        // owned-input path), so `Arc::<str>::from(v)` returns the same
        // bytes `v.as_str()` borrows and round-trips byte-equal through
        // the paired forward [`From<String> for CaixaVersion`]
        // constructor closing the two-way `Self → Arc<str> → Self`
        // cycle by construction. Refuses any future silent detour that
        // would swap `Arc::<str>::from(v.0)` for an allocating
        // `.as_str().to_owned().into()` cascade (the pre-lift compose
        // shape would double-allocate a fresh intermediary [`String`]
        // on the way to the same [`Arc<str>`] slot), a stray
        // `.trim().to_owned().into()` normalization, or a routing
        // through the sibling [`fmt::Display`] emitter that would
        // introduce a formatter round-trip.
        use std::sync::Arc;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let expected = v.as_str().to_owned();
            let arced: Arc<str> = Arc::<str>::from(v.clone());
            assert_eq!(
                arced.as_ref(),
                expected.as_str(),
                "Arc::<str>::from(v) must return the wrapper's own bytes verbatim",
            );
            let round_trip: CaixaVersion = arced.as_ref().to_owned().into();
            assert_eq!(
                round_trip, v,
                "Arc::<str>::from(v) must round-trip byte-equal through \
                 the From<String> constructor",
            );
        }
    }

    #[test]
    fn caixa_version_from_into_owned_arc_str_and_string_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the owned-
        // input [`From<CaixaVersion> for std::sync::Arc<str>`] reverse
        // projection and the paired owned-input
        // [`From<CaixaVersion> for String`],
        // [`From<CaixaVersion> for Cow<'static, str>`], and
        // [`From<CaixaVersion> for Box<str>`] reverse projections
        // resolve to the same bytes on every instance, and all four
        // agree with the borrowed [`AsRef<str>`] surface on the same
        // wrapped body. Refuses any future silent split between the
        // four owned-input reverse-projection axes (a stray
        // normalization on one path only, a divergent routing that
        // would let `Arc::<str>::from(v.clone())`,
        // `Box::<str>::from(v.clone())`, `String::from(v.clone())`,
        // and `Cow::from(v.clone())` disagree on the same body) that
        // would silently split the same-shape owned-move discipline
        // across the four reverse-projection targets.
        use std::borrow::Cow;
        use std::sync::Arc;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_string: String = String::from(v.clone());
            let via_cow: Cow<'static, str> = Cow::from(v.clone());
            let via_box: Box<str> = Box::<str>::from(v.clone());
            let via_arc: Arc<str> = Arc::<str>::from(v.clone());
            let via_as_ref: &str = <CaixaVersion as AsRef<str>>::as_ref(&v);
            assert_eq!(via_arc.as_ref(), via_string.as_str());
            assert_eq!(via_arc.as_ref(), via_cow.as_ref());
            assert_eq!(via_arc.as_ref(), via_box.as_ref());
            assert_eq!(via_arc.as_ref(), via_as_ref);
            assert_eq!(via_arc.as_ref(), versao);
        }
    }

    #[test]
    fn caixa_version_from_borrowed_into_owned_arc_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<&CaixaVersion> for std::sync::Arc<str>` — asserts
        // the borrowed-input reverse projection allocates a fresh
        // [`std::sync::Arc<str>`] whose bytes byte-equal the
        // substrate-primitive [`CaixaVersion::as_str`] accessor on the
        // same instance, preserving the source [`CaixaVersion`] intact
        // (no move-out). Refuses any future silent detour that would
        // route the impl through a divergent projection (a stray
        // normalization step, a swap onto the sibling [`fmt::Display`]-
        // routed [`ToString::to_string`] surface followed by
        // `.into()`, a re-inlining that dereferences `&self.0` outside
        // the shared accessor).
        use std::sync::Arc;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Arc<str> = Arc::<str>::from(&v);
            assert_eq!(
                via_borrowed.as_ref(),
                v.as_str(),
                "Arc::<str>::from(&v) must byte-equal CaixaVersion::as_str",
            );
            // The borrowed-input impl must not move out of the source.
            assert_eq!(
                v.as_str(),
                versao,
                "source CaixaVersion must survive borrowed-input projection",
            );
        }
    }

    #[test]
    fn caixa_version_from_owned_and_borrowed_into_arc_str_agree_on_every_shape() {
        // Fail-before-pass-after cross-corner partition pin: the paired
        // owned-input [`From<CaixaVersion> for std::sync::Arc<str>`]
        // and borrowed-input [`From<&CaixaVersion> for std::sync::Arc<str>`]
        // impls resolve to the same bytes on every instance, closing
        // the "owned-input move vs. borrowed-input clone" bifurcation
        // on the same wrapped body through the [`std::sync::Arc<str>`]
        // axis. Refuses any future silent split between the two
        // corners (a normalization on one path only, a divergent
        // routing that would let `Arc::<str>::from(v.clone())` and
        // `Arc::<str>::from(&v)` disagree on the same body).
        use std::sync::Arc;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Arc<str> = Arc::<str>::from(&v);
            let via_owned: Arc<str> = Arc::<str>::from(v.clone());
            assert_eq!(via_owned.as_ref(), via_borrowed.as_ref());
            assert_eq!(via_borrowed.as_ref(), versao);
        }
    }

    #[test]
    fn caixa_version_from_into_owned_rc_str_returns_wrapped_body() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<CaixaVersion> for std::rc::Rc<str>` — asserts the
        // owned-input reverse projection routes the wrapper's own
        // [`String`] body through [`std::rc::Rc::<str>::from`] verbatim
        // (one heap allocation of the single-threaded-refcounted slab, no
        // intermediary [`String`] or [`Box<str>`] on the owned-input
        // path), so `Rc::<str>::from(v)` returns the same bytes
        // `v.as_str()` borrows and round-trips byte-equal through the
        // paired forward [`From<String> for CaixaVersion`] constructor
        // closing the two-way `Self → Rc<str> → Self` cycle by
        // construction. Refuses any future silent detour that would swap
        // `Rc::<str>::from(v.0)` for an allocating
        // `.as_str().to_owned().into()` cascade (the pre-lift compose
        // shape would double-allocate a fresh intermediary [`String`] on
        // the way to the same [`Rc<str>`] slot), a stray
        // `.trim().to_owned().into()` normalization, or a routing through
        // the sibling [`fmt::Display`] emitter that would introduce a
        // formatter round-trip.
        use std::rc::Rc;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let expected = v.as_str().to_owned();
            let rced: Rc<str> = Rc::<str>::from(v.clone());
            assert_eq!(
                rced.as_ref(),
                expected.as_str(),
                "Rc::<str>::from(v) must return the wrapper's own bytes verbatim",
            );
            let round_trip: CaixaVersion = rced.as_ref().to_owned().into();
            assert_eq!(
                round_trip, v,
                "Rc::<str>::from(v) must round-trip byte-equal through \
                 the From<String> constructor",
            );
        }
    }

    #[test]
    fn caixa_version_from_into_owned_rc_str_and_arc_str_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the owned-
        // input [`From<CaixaVersion> for std::rc::Rc<str>`] reverse
        // projection and the paired owned-input
        // [`From<CaixaVersion> for String`],
        // [`From<CaixaVersion> for Cow<'static, str>`],
        // [`From<CaixaVersion> for Box<str>`], and
        // [`From<CaixaVersion> for std::sync::Arc<str>`] reverse
        // projections resolve to the same bytes on every instance, and
        // all five agree with the borrowed [`AsRef<str>`] surface on the
        // same wrapped body. Refuses any future silent split between the
        // five owned-input reverse-projection axes (a stray normalization
        // on one path only, a divergent routing that would let
        // `Rc::<str>::from(v.clone())`, `Arc::<str>::from(v.clone())`,
        // `Box::<str>::from(v.clone())`, `String::from(v.clone())`, and
        // `Cow::from(v.clone())` disagree on the same body) that would
        // silently split the same-shape owned-move discipline across the
        // five reverse-projection targets.
        use std::borrow::Cow;
        use std::rc::Rc;
        use std::sync::Arc;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let owned_string: String = String::from(v.clone());
            let owned_cow: Cow<'static, str> = Cow::from(v.clone());
            let owned_box: Box<str> = Box::<str>::from(v.clone());
            let atomic_handle: Arc<str> = Arc::<str>::from(v.clone());
            let single_handle: Rc<str> = Rc::<str>::from(v.clone());
            let borrowed_as_ref: &str = <CaixaVersion as AsRef<str>>::as_ref(&v);
            assert_eq!(single_handle.as_ref(), owned_string.as_str());
            assert_eq!(single_handle.as_ref(), owned_cow.as_ref());
            assert_eq!(single_handle.as_ref(), owned_box.as_ref());
            assert_eq!(single_handle.as_ref(), atomic_handle.as_ref());
            assert_eq!(single_handle.as_ref(), borrowed_as_ref);
            assert_eq!(single_handle.as_ref(), versao);
        }
    }

    #[test]
    fn caixa_version_from_borrowed_into_owned_rc_str_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<&CaixaVersion> for std::rc::Rc<str>` — asserts the
        // borrowed-input reverse projection allocates a fresh
        // [`std::rc::Rc<str>`] whose bytes byte-equal the substrate-
        // primitive [`CaixaVersion::as_str`] accessor on the same
        // instance, preserving the source [`CaixaVersion`] intact (no
        // move-out). Refuses any future silent detour that would route
        // the impl through a divergent projection (a stray normalization
        // step, a swap onto the sibling [`fmt::Display`]-routed
        // [`ToString::to_string`] surface followed by `.into()`, a
        // re-inlining that dereferences `&self.0` outside the shared
        // accessor).
        use std::rc::Rc;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Rc<str> = Rc::<str>::from(&v);
            assert_eq!(
                via_borrowed.as_ref(),
                v.as_str(),
                "Rc::<str>::from(&v) must byte-equal CaixaVersion::as_str",
            );
            // The borrowed-input impl must not move out of the source.
            assert_eq!(
                v.as_str(),
                versao,
                "source CaixaVersion must survive borrowed-input projection",
            );
        }
    }

    #[test]
    fn caixa_version_from_owned_and_borrowed_into_rc_str_agree_on_every_shape() {
        // Fail-before-pass-after cross-corner partition pin: the paired
        // owned-input [`From<CaixaVersion> for std::rc::Rc<str>`] and
        // borrowed-input [`From<&CaixaVersion> for std::rc::Rc<str>`]
        // impls resolve to the same bytes on every instance, closing the
        // "owned-input move vs. borrowed-input clone" bifurcation on the
        // same wrapped body through the [`std::rc::Rc<str>`] axis.
        // Refuses any future silent split between the two corners (a
        // normalization on one path only, a divergent routing that would
        // let `Rc::<str>::from(v.clone())` and `Rc::<str>::from(&v)`
        // disagree on the same body).
        use std::rc::Rc;
        for versao in ["0.1.0", "1.2.3-alpha.1", "0.0.0", ""] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Rc<str> = Rc::<str>::from(&v);
            let via_owned: Rc<str> = Rc::<str>::from(v.clone());
            assert_eq!(via_owned.as_ref(), via_borrowed.as_ref());
            assert_eq!(via_borrowed.as_ref(), versao);
        }
    }

    #[test]
    fn caixa_version_from_str_routes_through_from_str_reference_impl() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl std::str::FromStr for CaixaVersion` — asserts the
        // stdlib parse-set entry point delegates byte-for-byte through
        // the paired borrowed-input `impl From<&str> for CaixaVersion`
        // constructor above (which wraps `s.to_string()` into the
        // newtype's inner `String` slot), so every consumer that reaches
        // [`CaixaVersion`] through the standard-library `T: FromStr`-
        // bounded parse surface (`str::parse::<CaixaVersion>`, the
        // `<CaixaVersion as std::str::FromStr>::from_str` explicit-trait
        // spelling on a generic bound, a `clap::value_parser!(CaixaVersion)`
        // short-form on a future arg-parse, a `serde_with::DisplayFromStr`
        // wrapper on a downstream typed-YAML derive) routes through the
        // same wrap the sibling `From<&str>` forward-projection already
        // installs. Refuses any future silent detour that would route
        // the stdlib entry point through a divergent projection (a stray
        // `parse_semver_first` validation gate slipping onto the wrap
        // path, a normalization step that would drop whitespace or
        // canonicalize a prerelease tag, a swap onto the paired
        // [`CaixaVersion::parse`] `Result<semver::Version, VersionError>`
        // accessor that would narrow the accept-set to the semver
        // grammar's shape ahead of the wrap). Also witnesses the
        // `Err = Infallible` type-level shape at compile time under the
        // explicit `Result<CaixaVersion, std::convert::Infallible>`
        // annotation the loop body binds against — any future accidental
        // widening of the error type (a swap onto `type Err =
        // VersionError`) trips the annotation at caixa-core build time
        // with E0308 (`expected Infallible, found <T>`), strictly stronger
        // than a runtime `.unwrap()` on the sibling `.parse()` short-form.
        //
        // Sweeps the same fixture bodies the sibling
        // `caixa_version_as_str_accessor_is_const_fn` /
        // `caixa_version_from_owned_and_borrowed_into_rc_str_agree_on_every_shape`
        // pins already cover on the paired scalar-accessor and
        // owned-vs-borrowed axes: canonical semver, prerelease-shape,
        // zero-body, and empty-string corners. Extends the sweep with a
        // requirement-shape (`^0.1`), a star (`*`), and a non-semver junk
        // body (`not-a-version`) so the total-wrap discipline the
        // `Err = Infallible` shape witnesses is asserted across the four
        // canonical axes of the input space: semver-shaped bodies the
        // paired [`CaixaVersion::parse`] accessor would accept, semver-
        // requirement-shaped bodies the sibling [`parse_requirement`]
        // surface consumes, empty bodies (which the wrap accepts but
        // downstream semver rejects), and non-semver junk (which the
        // wrap accepts and downstream semver rejects).
        use std::str::FromStr;
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
        ] {
            let via_parse_short_form: Result<CaixaVersion, std::convert::Infallible> =
                versao.parse::<CaixaVersion>();
            let via_from_str_explicit: Result<CaixaVersion, std::convert::Infallible> =
                <CaixaVersion as FromStr>::from_str(versao);
            let via_from_ref: CaixaVersion = <CaixaVersion as From<&str>>::from(versao);
            let via_parse_ok: CaixaVersion = via_parse_short_form.unwrap();
            let via_from_str_ok: CaixaVersion = via_from_str_explicit.unwrap();
            assert_eq!(
                via_parse_ok.as_str(),
                versao,
                "str::parse::<CaixaVersion>() must byte-equal the input",
            );
            assert_eq!(
                via_from_str_ok.as_str(),
                versao,
                "<CaixaVersion as FromStr>::from_str must byte-equal the input",
            );
            assert_eq!(
                via_parse_ok, via_from_ref,
                "str::parse::<CaixaVersion>() must byte-equal From<&str>",
            );
            assert_eq!(
                via_from_str_ok, via_from_ref,
                "<CaixaVersion as FromStr>::from_str must byte-equal From<&str>",
            );
        }
    }

    #[test]
    fn caixa_version_from_str_round_trips_through_display_on_every_input() {
        // Fail-before-pass-after round-trip pin: the lifted
        // `impl std::str::FromStr for CaixaVersion` closes the two-way
        // canonical wire-form axis with the paired forward-projection
        // [`fmt::Display`] impl at line 29 —
        // `s.parse::<CaixaVersion>().unwrap().to_string() == s` for every
        // `&str` on the total-wrap axis the newtype installs at rest.
        // Refuses any future silent narrowing on either half (a stray
        // normalization step slipping onto the [`fmt::Display`] impl that
        // would canonicalize the wrapped body ahead of `f.write_str`, a
        // divergent wrap on the `FromStr` impl that would swap the paired
        // `From<&str>` constructor for a fresh `String::from(s).trim()`-
        // style body-mutating projection) so the round-trip theorem the
        // pin states remains machine-checked at caixa-core test time.
        //
        // Peer of the sibling `caixa_version_from_str_routes_through_from_str_reference_impl`
        // pin immediately above (which witnesses the byte-parity axis
        // against the paired `From<&str>` forward-projection); this pin
        // witnesses the same axis against the paired `fmt::Display`
        // forward-projection instead, closing the two-way round-trip
        // through the standard-library [`fmt::Display`] / [`FromStr`]
        // pair the substrate reaches [`CaixaVersion`] through on the
        // canonical wire-form axis.
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let parsed: CaixaVersion = versao.parse::<CaixaVersion>().unwrap();
            let displayed: String = parsed.to_string();
            assert_eq!(
                displayed, versao,
                "CaixaVersion::from_str + Display must round-trip byte-for-byte",
            );
        }
    }

    #[test]
    fn caixa_version_as_ref_bytes_routes_through_as_str_accessor() {
        // `<T: AsRef<[u8]>>`-bound-consumer witness helper: a generic
        // byte-input function accepts a [`CaixaVersion`] directly through
        // the trait bound, without the caller open-coding the two-hop
        // `v.as_str().as_bytes()` composition. Lifted to the top of the
        // function per `clippy::items_after_statements`.
        fn generic_bytes_sink<T: AsRef<[u8]>>(t: T) -> Vec<u8> {
            t.as_ref().to_vec()
        }
        // `blake3::Hasher::update`-shape byte-input surface mock: mirrors
        // `blake3::Hasher::update` / `ring::digest::Context::update` /
        // `sha2::Sha256::update`'s `impl AsRef<[u8]>`-bound `update`
        // signature so a per-`:versao` BLAKE3 content-address closure
        // that composes `hasher.update(caixa.versao())` on the
        // [`crate::Lacre`] closure builder reaches the substrate-
        // primitive [`CaixaVersion::as_str`] accessor through this axis
        // and no other.
        struct MockHasher(Vec<u8>);
        impl MockHasher {
            fn new() -> Self {
                Self(Vec::new())
            }
            fn update(&mut self, bytes: impl AsRef<[u8]>) -> &mut Self {
                self.0.extend_from_slice(bytes.as_ref());
                self
            }
            fn finalize(self) -> Vec<u8> {
                self.0
            }
        }

        // Fail-before-pass-after byte-parity pin on the newly lifted
        // `impl AsRef<[u8]> for CaixaVersion` — asserts the trait-
        // idiomatic byte-view standard-library impl and the substrate-
        // primitive [`CaixaVersion::as_str`] `pub const fn` accessor's
        // `.as_bytes()` byte-tail resolve to the same byte-string across
        // every canonical `:versao`-shaped input the sibling
        // `caixa_version_as_str_accessor_is_const_fn` and
        // `caixa_version_from_str_round_trips_through_display_on_every_input`
        // pins already sweep (canonical semver, prerelease-shape, zero-
        // body, empty-string, requirement-shape, star, non-semver junk,
        // build-metadata-tail). Opens the trait-idiomatic byte-view axis
        // on the substrate's core String-wrapper newtype primitive
        // [`CaixaVersion`], mirroring the paired [`AsRef<[u8]>`] axis
        // every closed-set fieldless typed enum peer already carries.
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_trait: &[u8] = <CaixaVersion as AsRef<[u8]>>::as_ref(&v);
            let via_method_bytes: &[u8] = v.as_str().as_bytes();
            assert_eq!(
                via_trait, via_method_bytes,
                "AsRef<[u8]> for CaixaVersion impl must byte-equal \
                 CaixaVersion::as_str().as_bytes() on {versao:?} — \
                 divergence signals a silent detour off the substrate-\
                 primitive accessor",
            );
            // Cross-axis witness against the paired str-view axes'
            // `.as_bytes()` byte-tails: the newly lifted byte-view
            // impl and every str-view axis on the same primitive
            // ([`AsRef<str>`], [`fmt::Display`], [`CaixaVersion::as_str`])
            // must resolve to the same byte-tail by construction — any
            // future silent split at the substrate-primitive accessor
            // trips here rather than at a downstream consumer.
            let str_view_ref: &str = <CaixaVersion as AsRef<str>>::as_ref(&v);
            assert_eq!(
                via_trait,
                str_view_ref.as_bytes(),
                "AsRef<[u8]> and AsRef<str> for CaixaVersion must resolve \
                 to byte-equal byte-tails on {versao:?}",
            );
            let display_bytes = v.to_string();
            assert_eq!(
                via_trait,
                display_bytes.as_bytes(),
                "AsRef<[u8]> and <CaixaVersion as fmt::Display>::to_string \
                 must resolve to byte-equal byte-tails on {versao:?}",
            );
            let borrow_view: &str = <CaixaVersion as std::borrow::Borrow<str>>::borrow(&v);
            assert_eq!(
                via_trait,
                borrow_view.as_bytes(),
                "AsRef<[u8]> and Borrow<str> for CaixaVersion must resolve \
                 to byte-equal byte-tails on {versao:?}",
            );
            // `<T: AsRef<[u8]>>`-bound-consumer witness: a generic
            // byte-input function accepts the wrapper directly and
            // returns the same bytes the substrate-primitive accessor's
            // `.as_bytes()` byte-tail carries.
            let sunk_owned: Vec<u8> = generic_bytes_sink(v.clone());
            assert_eq!(sunk_owned, via_method_bytes);
            let sunk_borrowed: Vec<u8> = generic_bytes_sink(&v);
            assert_eq!(sunk_borrowed, via_method_bytes);
            // `blake3::Hasher::update`-shape witness: the
            // per-`:versao` BLAKE3 content-address closure builder reaches
            // the substrate-primitive accessor through the byte-view axis
            // and folds the same bytes the paired str-view axes surface.
            let mut mock_hasher = MockHasher::new();
            mock_hasher.update(&v);
            let folded_bytes = mock_hasher.finalize();
            assert_eq!(folded_bytes, via_method_bytes);
        }
    }

    #[test]
    fn caixa_version_from_into_owned_vec_bytes_returns_wrapped_body() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<CaixaVersion> for Vec<u8>` — asserts the owned-input
        // byte-owned forward-projection routes the wrapper's own heap
        // allocation through [`String::into_bytes`] verbatim (no re-copy
        // of the wrapped body's bytes, no normalization detour) so
        // `Vec::<u8>::from(v)` returns the same bytes `v.as_str()`
        // borrows via `.as_bytes()`. Refuses any future silent detour
        // that would swap the move on `v.0.into_bytes()` for an
        // allocating `.as_str().as_bytes().to_vec()` cascade (the
        // pre-lift compose shape), a `String::from(v).into_bytes()`
        // two-hop reverse-then-move shape, a stray
        // `.trim().as_bytes().to_vec()` normalization, or a routing
        // through the sibling [`fmt::Display`] emitter that would
        // introduce a formatter round-trip. Closes the byte-owned
        // forward-projection axis in lockstep with the paired str-owned
        // [`From<CaixaVersion> for String`] axis (which routes through
        // `v.0`).
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let expected: Vec<u8> = v.as_str().as_bytes().to_vec();
            let owned: Vec<u8> = Vec::<u8>::from(v);
            assert_eq!(
                owned, expected,
                "Vec::<u8>::from(v) must return the wrapper's own bytes verbatim on {versao:?}",
            );
            assert_eq!(
                owned,
                versao.as_bytes(),
                "Vec::<u8>::from(v) must byte-equal the pre-lift wrapped \
                 String storage on round-trip through the From<&str> constructor",
            );
            // Round-trip witness through the paired forward constructor:
            // the emitted owned byte-tail rematerializes into a
            // [`String`] via [`String::from_utf8`] and folds through
            // the paired [`From<String> for CaixaVersion`] constructor
            // to the same [`CaixaVersion`] value — closes the
            // `Self → Vec<u8> → String → Self` round-trip whenever the
            // wrapped body is valid UTF-8, which every `SemVer`-shaped
            // `:versao` body is by construction (semver's grammar is
            // ASCII-only).
            if let Ok(round) = String::from_utf8(owned) {
                let back: CaixaVersion = round.into();
                assert_eq!(back.as_str(), versao);
            }
        }
    }

    #[test]
    fn caixa_version_from_borrowed_into_owned_vec_bytes_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<&CaixaVersion> for Vec<u8>` — asserts the
        // borrowed-input byte-owned forward-projection allocates a fresh
        // [`Vec<u8>`] whose bytes byte-equal the substrate-primitive
        // [`CaixaVersion::as_str`] accessor's `.as_bytes()` byte-tail
        // (via [`slice::to_vec`]) so `Vec::<u8>::from(&v)` returns the
        // same bytes without consuming the source wrapper. Refuses any
        // future silent detour: a stray normalization step that would
        // drop whitespace or canonicalize a prerelease tag ahead of the
        // byte-owned emit, a swap onto `String::from(v.clone()).into_bytes()`
        // that would spuriously clone the intermediate [`String`], or a
        // routing through the sibling [`fmt::Display`] emitter that
        // would introduce a formatter round-trip. Includes a source-
        // survival witness — the borrowed input remains readable after
        // the projection returns, confirming the impl takes only a
        // borrow and does not silently move out of the source.
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Vec<u8> = Vec::<u8>::from(&v);
            assert_eq!(
                via_borrowed,
                v.as_str().as_bytes(),
                "Vec::<u8>::from(&v) must byte-equal CaixaVersion::as_str().as_bytes() \
                 on {versao:?} — divergence signals a silent detour off \
                 the substrate-primitive accessor",
            );
            // Source-survival witness: `v` is borrowed, not moved, so
            // the pre-existing borrow-projection axes stay reachable
            // through the same instance after the byte-owned projection
            // returns.
            assert_eq!(v.as_str(), versao);
        }
    }

    #[test]
    fn caixa_version_from_owned_and_borrowed_into_vec_bytes_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the paired
        // owned-input [`From<CaixaVersion> for Vec<u8>`] and
        // borrowed-input [`From<&CaixaVersion> for Vec<u8>`] impls
        // resolve to the same byte-tail on every instance, and both
        // agree byte-for-byte with the pre-existing byte-view
        // [`AsRef<[u8]>`] axis on the same primitive. Closes the
        // "owned-input move vs. borrowed-input clone" bifurcation on
        // the [`Vec<u8>`] axis and the "owned-heap vs. borrowed-view"
        // bifurcation between the byte-owned forward-projection and the
        // pre-existing borrow-projection byte-view axis. Refuses any
        // future silent split between the two corners (a normalization
        // on one path only, a divergent routing that would let
        // `Vec::<u8>::from(v.clone())` and `Vec::<u8>::from(&v)`
        // disagree, or a swap on `AsRef::<[u8]>::as_ref` that would
        // diverge from either projection).
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_owned: Vec<u8> = Vec::<u8>::from(v.clone());
            let via_borrowed: Vec<u8> = Vec::<u8>::from(&v);
            let via_as_ref: &[u8] = <CaixaVersion as AsRef<[u8]>>::as_ref(&v);
            assert_eq!(via_owned, via_borrowed);
            assert_eq!(via_borrowed.as_slice(), via_as_ref);
            assert_eq!(via_borrowed, versao.as_bytes());
        }
    }

    #[test]
    fn caixa_version_from_into_owned_cow_bytes_returns_owned_wrapped_body() {
        // Fail-before-pass-after byte-parity + [`Cow::Owned`]-arm pin
        // on the lifted `impl From<CaixaVersion> for
        // std::borrow::Cow<'static, [u8]>` — asserts the owned-input
        // byte-owned reverse projection routes the wrapper's own heap
        // allocation through `Cow::Owned(v.0.into_bytes())` verbatim
        // (no re-copy, no normalization detour, no `Cow::Borrowed`
        // misclassification that would demand a `&'static [u8]` the
        // runtime wrapper cannot carry), so the emitted [`Cow`] byte-
        // equals the substrate-primitive [`CaixaVersion::as_str`]
        // accessor's `.as_bytes()` byte-tail on the same instance and
        // round-trips byte-equal through the paired forward
        // [`From<String> for CaixaVersion`] constructor after
        // re-materializing through [`String::from_utf8`]. Refuses any
        // future silent detour: a swap of the move on `v.0.into_bytes()`
        // for an allocating `.as_str().as_bytes().to_vec()` cascade
        // (the pre-lift compose shape would double-allocate a fresh
        // intermediary [`Vec<u8>`] on the way to the same
        // [`Cow::Owned`] arm), a stray `.trim().as_bytes().to_vec()`
        // normalization, or a mis-routing through [`Cow::Borrowed`] on
        // a non-`'static` byte-string that would not type-check.
        use std::borrow::Cow;
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let expected: Vec<u8> = v.as_str().as_bytes().to_vec();
            let cow: Cow<'static, [u8]> = Cow::from(v.clone());
            assert!(
                matches!(cow, Cow::Owned(_)),
                "From<CaixaVersion> for Cow<'static, [u8]> must land on \
                 the Cow::Owned arm — a runtime String wrapper cannot \
                 promise the 'static lifetime the Cow::Borrowed arm \
                 requires on {versao:?}",
            );
            assert_eq!(
                cow.as_ref(),
                expected.as_slice(),
                "Cow::from(v) must return the wrapper's own bytes verbatim on {versao:?}",
            );
            assert_eq!(
                cow.as_ref(),
                versao.as_bytes(),
                "Cow::from(v) must byte-equal the pre-lift wrapped String \
                 storage on round-trip through the From<&str> constructor",
            );
            // Round-trip witness through the paired forward constructor:
            // the emitted owned byte-tail rematerializes into a
            // [`String`] via [`String::from_utf8`] and folds through
            // the paired [`From<String> for CaixaVersion`] constructor
            // to the same [`CaixaVersion`] value — closes the
            // `Self → Cow<'static, [u8]> → Vec<u8> → String → Self`
            // round-trip whenever the wrapped body is valid UTF-8, which
            // every `SemVer`-shaped `:versao` body is by construction
            // (semver's grammar is ASCII-only).
            if let Ok(round) = String::from_utf8(cow.into_owned()) {
                let back: CaixaVersion = round.into();
                assert_eq!(back.as_str(), versao);
            }
        }
    }

    #[test]
    fn caixa_version_from_into_owned_cow_bytes_and_vec_bytes_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the owned-
        // input [`From<CaixaVersion> for Cow<'static, [u8]>`] byte-owned
        // reverse projection and the paired owned-input
        // [`From<CaixaVersion> for Vec<u8>`] byte-owned reverse
        // projection resolve to the same bytes on every instance, and
        // both agree with the pre-existing borrowed [`AsRef<[u8]>`]
        // byte-view axis on the same wrapped body. Refuses any future
        // silent split between the two owned-input byte-owned reverse-
        // projection axes (a stray normalization on one path only, a
        // divergent routing that would let `Cow::from(v.clone())` and
        // `Vec::<u8>::from(v.clone())` disagree on the same body) that
        // would silently split the same-shape owned-move discipline
        // across the two byte-family reverse-projection targets.
        use std::borrow::Cow;
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_vec: Vec<u8> = Vec::<u8>::from(v.clone());
            let via_cow: Cow<'static, [u8]> = Cow::from(v.clone());
            let via_as_ref: &[u8] = <CaixaVersion as AsRef<[u8]>>::as_ref(&v);
            assert_eq!(via_cow.as_ref(), via_vec.as_slice());
            assert_eq!(via_cow.as_ref(), via_as_ref);
            assert_eq!(via_cow.as_ref(), versao.as_bytes());
        }
    }

    #[test]
    fn caixa_version_from_borrowed_into_owned_cow_bytes_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity + [`Cow::Owned`]-arm pin
        // on the lifted `impl From<&CaixaVersion> for
        // std::borrow::Cow<'static, [u8]>` — asserts the borrowed-input
        // byte-owned reverse projection allocates a fresh
        // [`Cow::Owned`] whose bytes byte-equal the substrate-primitive
        // [`CaixaVersion::as_str`] accessor's `.as_bytes()` byte-tail
        // on the same instance (via [`slice::to_vec`]), preserving the
        // source [`CaixaVersion`] intact (no move-out). Refuses any
        // future silent detour that would route the impl through a
        // divergent projection (a stray normalization step that would
        // drop whitespace or canonicalize a prerelease tag ahead of the
        // byte-owned emit, a mis-routing onto [`Cow::Borrowed`] on a
        // non-`'static` byte-string that would not type-check, a
        // re-inlining that dereferences `&self.0` outside the shared
        // accessor).
        use std::borrow::Cow;
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Cow<'static, [u8]> = Cow::from(&v);
            assert!(
                matches!(via_borrowed, Cow::Owned(_)),
                "From<&CaixaVersion> for Cow<'static, [u8]> must land on \
                 the Cow::Owned arm — a runtime String wrapper cannot \
                 promise the 'static lifetime the Cow::Borrowed arm \
                 requires on {versao:?}",
            );
            assert_eq!(
                via_borrowed.as_ref(),
                v.as_str().as_bytes(),
                "Cow::from(&v) must byte-equal CaixaVersion::as_str().as_bytes() \
                 on {versao:?} — divergence signals a silent detour off \
                 the substrate-primitive accessor",
            );
            // The borrowed-input impl must not move out of the source.
            assert_eq!(
                v.as_str(),
                versao,
                "source CaixaVersion must survive borrowed-input projection",
            );
        }
    }

    #[test]
    fn caixa_version_from_owned_and_borrowed_into_cow_bytes_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the paired
        // owned-input [`From<CaixaVersion> for Cow<'static, [u8]>`] and
        // borrowed-input [`From<&CaixaVersion> for Cow<'static, [u8]>`]
        // impls resolve to the same bytes on every instance, closing
        // the "owned-input move vs. borrowed-input clone" bifurcation
        // on the same wrapped body through the [`Cow<'static, [u8]>`]
        // axis. Refuses any future silent split between the two corners
        // (a normalization on one path only, a divergent routing that
        // would let `Cow::from(v.clone())` and `Cow::from(&v)` disagree
        // on the same body). Both corners must land on [`Cow::Owned`]
        // — the runtime wrapper's storage rules out the borrowed arm
        // on both input shapes alike.
        use std::borrow::Cow;
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Cow<'static, [u8]> = Cow::from(&v);
            let via_owned: Cow<'static, [u8]> = Cow::from(v.clone());
            let via_as_ref: &[u8] = <CaixaVersion as AsRef<[u8]>>::as_ref(&v);
            assert!(matches!(via_borrowed, Cow::Owned(_)));
            assert!(matches!(via_owned, Cow::Owned(_)));
            assert_eq!(via_owned.as_ref(), via_borrowed.as_ref());
            assert_eq!(via_borrowed.as_ref(), via_as_ref);
            assert_eq!(via_borrowed.as_ref(), versao.as_bytes());
        }
    }

    #[test]
    fn caixa_version_from_into_owned_box_bytes_returns_wrapped_body() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<CaixaVersion> for Box<[u8]>` — asserts the owned-
        // input byte-owned reverse projection routes the wrapper's own
        // heap allocation through
        // `v.0.into_bytes().into_boxed_slice()` verbatim (no re-copy of
        // the underlying bytes on the fixed-capacity path;
        // `Vec::<u8>::into_boxed_slice` reuses the same allocation when
        // length matches capacity), so `Box::<[u8]>::from(v)` returns
        // the same bytes `v.as_str().as_bytes()` borrows and round-trips
        // byte-equal through the paired forward
        // [`From<String> for CaixaVersion`] constructor after re-
        // materializing the boxed byte-tail through
        // [`String::from_utf8`] on every UTF-8-valid `:versao` body
        // (which every `SemVer`-shaped body is by construction).
        // Refuses any future silent detour that would swap
        // `v.0.into_bytes().into_boxed_slice()` for an allocating
        // `.as_str().as_bytes().to_vec().into_boxed_slice()` cascade
        // (the pre-lift compose shape would double-allocate a fresh
        // intermediary [`Vec<u8>`] on the way to the same
        // [`Box<[u8]>`] slot), a stray `.trim().as_bytes()...`
        // normalization, or a routing through the sibling
        // [`fmt::Display`] emitter that would introduce a formatter
        // round-trip.
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let expected: Vec<u8> = v.as_str().as_bytes().to_vec();
            let boxed: Box<[u8]> = Box::<[u8]>::from(v.clone());
            assert_eq!(
                boxed.as_ref(),
                expected.as_slice(),
                "Box::<[u8]>::from(v) must return the wrapper's own bytes verbatim on {versao:?}",
            );
            assert_eq!(
                boxed.as_ref(),
                versao.as_bytes(),
                "Box::<[u8]>::from(v) must byte-equal the pre-lift wrapped \
                 String storage on round-trip through the From<&str> constructor",
            );
            // Round-trip witness through the paired forward constructor:
            // the emitted boxed byte-tail rematerializes into a
            // [`Vec<u8>`] via [`Box::<[u8]>::into_vec`], folds through
            // [`String::from_utf8`], and lands back on the same
            // [`CaixaVersion`] value via the paired
            // [`From<String> for CaixaVersion`] constructor — closing
            // the `Self → Box<[u8]> → Vec<u8> → String → Self`
            // round-trip whenever the wrapped body is valid UTF-8.
            if let Ok(round) = String::from_utf8(boxed.into_vec()) {
                let back: CaixaVersion = round.into();
                assert_eq!(back.as_str(), versao);
            }
        }
    }

    #[test]
    fn caixa_version_from_into_owned_box_bytes_and_vec_bytes_agree_on_every_shape() {
        // Fail-before-pass-after cross-axis partition pin: the owned-
        // input [`From<CaixaVersion> for Box<[u8]>`] byte-owned reverse
        // projection and the paired owned-input
        // [`From<CaixaVersion> for Vec<u8>`] and
        // [`From<CaixaVersion> for Cow<'static, [u8]>`] byte-owned
        // reverse projections resolve to the same bytes on every
        // instance, and all three agree with the pre-existing borrowed
        // [`AsRef<[u8]>`] byte-view axis on the same wrapped body.
        // Refuses any future silent split between the three owned-input
        // byte-owned reverse-projection axes (a stray normalization on
        // one path only, a divergent routing that would let
        // `Box::<[u8]>::from(v.clone())`, `Vec::<u8>::from(v.clone())`,
        // and `Cow::from(v.clone())` disagree on the same body) that
        // would silently split the same-shape owned-move discipline
        // across the three byte-family reverse-projection targets.
        use std::borrow::Cow;
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_vec: Vec<u8> = Vec::<u8>::from(v.clone());
            let via_cow: Cow<'static, [u8]> = Cow::from(v.clone());
            let via_box: Box<[u8]> = Box::<[u8]>::from(v.clone());
            let via_as_ref: &[u8] = <CaixaVersion as AsRef<[u8]>>::as_ref(&v);
            assert_eq!(via_box.as_ref(), via_vec.as_slice());
            assert_eq!(via_box.as_ref(), via_cow.as_ref());
            assert_eq!(via_box.as_ref(), via_as_ref);
            assert_eq!(via_box.as_ref(), versao.as_bytes());
        }
    }

    #[test]
    fn caixa_version_from_borrowed_into_owned_box_bytes_routes_through_as_str_accessor() {
        // Fail-before-pass-after byte-parity pin on the lifted
        // `impl From<&CaixaVersion> for Box<[u8]>` — asserts the
        // borrowed-input byte-owned reverse projection allocates a
        // fresh [`Box<[u8]>`] whose bytes byte-equal the substrate-
        // primitive [`CaixaVersion::as_str`] accessor's `.as_bytes()`
        // byte-tail on the same instance (via
        // [`Box::<[u8]>::from`]`(&[u8])`, which allocates a fit-to-
        // length boxed byte slice from the borrowed `&[u8]` in one
        // heap allocation without an intermediary [`Vec<u8>`]),
        // preserving the source [`CaixaVersion`] intact (no move-out).
        // Refuses any future silent detour that would route the impl
        // through a divergent projection (a stray normalization step
        // that would drop whitespace or canonicalize a prerelease tag
        // ahead of the byte-owned emit, a swap onto the sibling
        // [`fmt::Display`]-routed [`ToString::to_string`] surface
        // followed by `.into_bytes().into_boxed_slice()`, a re-inlining
        // that dereferences `&self.0` outside the shared accessor).
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Box<[u8]> = Box::<[u8]>::from(&v);
            assert_eq!(
                via_borrowed.as_ref(),
                v.as_str().as_bytes(),
                "Box::<[u8]>::from(&v) must byte-equal CaixaVersion::as_str().as_bytes() \
                 on {versao:?} — divergence signals a silent detour off \
                 the substrate-primitive accessor",
            );
            // The borrowed-input impl must not move out of the source.
            assert_eq!(
                v.as_str(),
                versao,
                "source CaixaVersion must survive borrowed-input projection",
            );
        }
    }

    #[test]
    fn caixa_version_from_owned_and_borrowed_into_box_bytes_agree_on_every_shape() {
        // Fail-before-pass-after cross-corner partition pin: the paired
        // owned-input [`From<CaixaVersion> for Box<[u8]>`] and
        // borrowed-input [`From<&CaixaVersion> for Box<[u8]>`] impls
        // resolve to the same bytes on every instance, closing the
        // "owned-input move vs. borrowed-input clone" bifurcation on
        // the same wrapped body through the [`Box<[u8]>`] axis.
        // Refuses any future silent split between the two corners (a
        // normalization on one path only, a divergent routing that
        // would let `Box::<[u8]>::from(v.clone())` and
        // `Box::<[u8]>::from(&v)` disagree on the same body).
        for versao in [
            "0.1.0",
            "1.2.3-alpha.1",
            "0.0.0",
            "",
            "^0.1",
            "*",
            "not-a-version",
            "1.2.3+build.42",
        ] {
            let v: CaixaVersion = versao.into();
            let via_borrowed: Box<[u8]> = Box::<[u8]>::from(&v);
            let via_owned: Box<[u8]> = Box::<[u8]>::from(v.clone());
            let via_as_ref: &[u8] = <CaixaVersion as AsRef<[u8]>>::as_ref(&v);
            assert_eq!(via_owned.as_ref(), via_borrowed.as_ref());
            assert_eq!(via_borrowed.as_ref(), via_as_ref);
            assert_eq!(via_borrowed.as_ref(), versao.as_bytes());
        }
    }
}
