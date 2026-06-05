use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single dependency declaration in a `caixa.lisp` manifest.
///
/// **Store model = Git, like Zig.** There is no central registry; a caixa is
/// just a Git repo with a `caixa.lisp` at its root. When `:fonte` is omitted,
/// the resolver falls back to `github:<default-org>/<nome>` (org defaults to
/// `pleme-io`, override via `~/.config/caixa/config.yaml`).
///
/// ```lisp
/// ;; Shorthand — resolves to github:pleme-io/caixa-teia (or your default org):
/// (:nome "caixa-teia" :versao "^0.1")
///
/// ;; Explicit git source:
/// (:nome "caixa-teia"
///  :versao "^0.1"
///  :fonte (:tipo git :repo "github:pleme-io/caixa-teia" :tag "v0.1.0"))
///
/// ;; Arbitrary git URL (not limited to GitHub):
/// (:nome "private-caixa"
///  :versao "*"
///  :fonte (:tipo git :repo "ssh://git@git.example/team/priv-caixa.git" :branch "main"))
///
/// ;; Local path (dev only; not publishable):
/// (:nome "caixa-teia"
///  :versao "0.1.0"
///  :fonte (:tipo path :caminho "../caixa-teia"))
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Dep {
    /// Caixa name — must match the target caixa's `:nome`.
    pub nome: String,

    /// Semver constraint string (`"^0.1"`, `"~0.1.2"`, `"0.1.0"`, `"*"`).
    pub versao: String,

    /// Where to fetch the caixa from. Defaults to the feira registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonte: Option<DepSource>,

    /// If true, a missing `:fonte` is not a build failure.
    #[serde(default, skip_serializing_if = "is_false")]
    pub opcional: bool,

    /// Feature flags to enable on the target caixa.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caracteristicas: Vec<String>,
}

/// Where a dep is fetched from. Tagged via `:tipo` in Lisp.
///
/// Only two shapes — Git and local Path. No central registry variant: a caixa
/// is just a Git repo. Omitting `:fonte` means *"use the default resolver
/// convention"*, which is `github:<default-org>/<nome>`; the resolver fills
/// that in when computing the lacre.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum DepSource {
    /// Clone from Git. One of `:tag`, `:rev`, or `:branch` may be set.
    /// `repo` can be a `github:org/repo` shorthand, a full `https://…` URL,
    /// or any git-ssh URL.
    Git {
        repo: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tag: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rev: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    /// Local filesystem path — dev only; cannot be published.
    Path { caminho: String },
}

impl DepSource {
    /// Build a registry-shorthand git source (`github:<org>/<nome>`).
    ///
    /// This is the resolver-side fallback for `dep.fonte: None`, not an
    /// author-surface value — it carries no pin (`:tag`/`:rev`/`:branch`
    /// all `None`) and is therefore rejected by [`Self::validate`]. The
    /// resolver fills the pin in at fetch time from the resolved commit;
    /// authors never serialize this shape as a `Dep::fonte` value.
    #[must_use]
    pub fn default_github(org: &str, nome: &str) -> Self {
        Self::Git {
            repo: format!("github:{org}/{nome}"),
            tag: None,
            rev: None,
            branch: None,
        }
    }

    /// Validate the `:fonte` value-shape: every author-surface
    /// `:fonte (:tipo git …)` must carry a non-empty `:repo` and
    /// exactly one of `:tag` / `:rev` / `:branch` set to a non-empty
    /// value; every `:fonte (:tipo path …)` must carry a non-empty
    /// `:caminho`.
    ///
    /// Called from [`Dep::validate`] with the dep's `:nome` so every
    /// diagnostic carries the offending entry verbatim — same
    /// self-locating shape the `:deps :versao` (2420c44),
    /// `:membros :versao` (9888b13), `:children :versao` (b38ff3a),
    /// `:placement :clusters` (6cbb900), and `:membros :caixa`
    /// (3f9d7a0) gates already expose.
    ///
    /// Until this gate landed `:fonte` was the only `:deps`-related
    /// typed surface still untyped past `Caixa::from_lisp`:
    /// - Empty `:repo` (`(:tipo git :repo "" :tag "v1")`) silently
    ///   passed parse and surfaced as a git-clone failure at
    ///   lacre-resolve time, far from the source caixa.lisp.
    /// - A bare `(:tipo git :repo "…")` with no `:tag`/`:rev`/`:branch`
    ///   passed parse and surfaced as the resolver's
    ///   [`ResolveError::MissingPin`](../../caixa-resolver/src/resolve.rs)
    ///   at fetch time, again far from the source caixa.lisp; lifting
    ///   to validate-time gives the author the same diagnostic at the
    ///   edit site.
    /// - `(:tipo git :repo "…" :tag "v1" :branch "main")` — multiple
    ///   pins set — passed parse and the resolver silently picked
    ///   `:rev > :tag > :branch`, ignoring the other pins with no
    ///   diagnostic; the author had no way to know their `:branch`
    ///   was dropped. This is the canonical "pin drift" footgun.
    /// - An empty pin value (`(:tipo git :repo "…" :tag "")`) silently
    ///   passed parse and surfaced as `git checkout ""` at fetch time.
    /// - Empty `:caminho` (`(:tipo path :caminho "")`) silently passed
    ///   parse and surfaced as
    ///   [`ResolveError::MissingPath`](../../caixa-resolver/src/resolve.rs)
    ///   with `path: PathBuf("")` — not actionable.
    ///
    /// Each rejected shape maps to a typed
    /// [`DepError::Fonte*`] variant that names the offending
    /// dep's `:nome` and the specific axis, so the author can grep
    /// their caixa.lisp for the `:nome "<nome>"` block and fix it in
    /// one edit.
    pub fn validate(&self, nome: &str) -> Result<(), DepError> {
        match self {
            Self::Git {
                repo,
                tag,
                rev,
                branch,
            } => {
                if repo.is_empty() {
                    return Err(DepError::FonteRepoEmpty {
                        nome: nome.to_string(),
                    });
                }
                // The `:repo` value flows verbatim into the caixa-resolver's
                // `git clone <repo>` subprocess invocation. Until this gate
                // landed `:repo` was the last untyped `:fonte`-related axis
                // past the empty arm: a malformed-but-non-empty repo URL
                // (`":repo "github:p/x ""` trailing space, paste-from-doc;
                // `":repo "-upload-pack=evil""` leading `-` — the canonical
                // CLI-argument-injection vector at the `git clone` boundary;
                // `":repo "pleme-io/caixa-teia""` missing scheme — `git clone`
                // reads as a relative filesystem path rather than the
                // GitHub-shorthand expansion; `":repo "github:p/x\n""`
                // embedded newline; `":repo "github:café/x""` raw non-ASCII)
                // silently passed validate and the failure surfaced at
                // lacre-resolve time with a porcelain-quoting-confused error
                // far from the source caixa.lisp. The lifted predicate makes
                // the git-porcelain-URL intersection-floor a substrate-level
                // invariant at validate time, peer with the three pin axes
                // (`:tag` + `:branch` via [`crate::render::is_git_ref_name`],
                // e70d213; `:rev` via [`crate::render::is_git_oid`], be07fd5)
                // — every `:fonte (:tipo git …)` past validate is now
                // structurally accept-shaped on every axis the resolver
                // consumes (the `:repo` URL the `git clone` invokes against,
                // the `:tag`/`:branch` refname `git fetch`/`git checkout`
                // accepts, the `:rev` commit OID the lacre's content-
                // addressing equality probe resolves), closing the
                // `:fonte` slot's value-shape trajectory end-to-end.
                if let Err(reason) = crate::render::is_git_repo_url(repo) {
                    return Err(DepError::FonteRepoShape {
                        nome: nome.to_string(),
                        repo: repo.clone(),
                        reason,
                    });
                }
                let pins: [(&'static str, Option<&String>); 3] = [
                    (":tag", tag.as_ref()),
                    (":rev", rev.as_ref()),
                    (":branch", branch.as_ref()),
                ];
                let set: Vec<&'static str> =
                    pins.iter().filter_map(|(n, v)| v.map(|_| *n)).collect();
                match set.len() {
                    0 => {
                        return Err(DepError::FontePinMissing {
                            nome: nome.to_string(),
                        });
                    }
                    1 => {
                        for (pin, value) in pins {
                            if value.is_some_and(String::is_empty) {
                                return Err(DepError::FontePinEmpty {
                                    nome: nome.to_string(),
                                    pin: pin.to_string(),
                                });
                            }
                        }
                    }
                    _ => {
                        return Err(DepError::FontePinAmbiguous {
                            nome: nome.to_string(),
                            pins: set.join(", "),
                        });
                    }
                }
                // Per-pin value-shape gate. The refname-shaped axes
                // (`:tag` + `:branch`) route through
                // [`crate::render::is_git_ref_name`]; the hex-OID-shaped
                // `:rev` axis routes through
                // [`crate::render::is_git_oid`]. The two predicates
                // partition the `:fonte` pin axes structurally — refname
                // vs. hex commit — so a cross-axis mis-slot (the
                // canonical "I conflated `:rev` and `:branch`" footgun:
                // `:rev "main"` defeating the reproducibility contract,
                // `:tag "deadbeef…"` mis-slotting a SHA into the
                // refname-shaped axis) lands at the offending axis's
                // predicate, not at lacre-resolve `git fetch` /
                // `git checkout` time. Their valid sets intersect at
                // the empty set: every refname is rejected by
                // `is_git_oid`, every OID is rejected by
                // `is_git_ref_name`, structurally.
                //
                // Until this gate landed `:tag` / `:branch` were the
                // refname-shaped axes still untyped past the empty-pin
                // arm: a malformed-but-non-empty refname
                // (`:tag "v0.1.0 "` trailing space — the canonical
                // paste-from-doc footgun; `:tag "v0.1.0.lock"` colliding
                // with git's atomic-rename guard suffix; `:tag "../escape"`
                // path-traversal via consecutive dots; `:branch "main "`
                // trailing space; `:branch "feature/foo bar"` embedded
                // space; `:branch "@"` the literal HEAD alias;
                // `:branch "refs/heads/main"` the fully-qualified ref
                // copied from `git show-ref` output that resolves to
                // a literal ref named `refs/heads/refs/heads/main` on
                // disk) silently passed validate; the `:rev` axis was
                // the last `:fonte`-related axis still untyped past the
                // empty-pin arm: a malformed-but-non-empty hex-OID
                // (`:rev "main"` conflating with `:branch` — the
                // reproducibility-contract leak; `:rev "v0.1.0"`
                // conflating with `:tag` — the same mis-slot on the
                // refname/OID boundary; `:rev "c0ffee"` an abbreviated
                // 6-char prefix that's ambiguous across repo history;
                // `:rev "DEADBEEF…"` an uppercase OID that round-trips
                // inconsistently against `git rev-parse HEAD`'s
                // lowercase emission) silently passed validate and the
                // failure surfaced at lacre-resolve `git fetch` /
                // `git checkout` time with a quoting-confused error
                // far from the source caixa.lisp, with no field naming
                // which `:deps` entry carried the typo. Lifting both
                // gates to caixa-build time matches the value-shape
                // trajectory the peer typed axes already follow
                // (c4213a4 typed WitContract endpoint/subject/slot;
                // eb3456d :entrada :paths; c7d05ec :entrada :host;
                // 4f0390b :contratos :endpoint; 6226bf4 :contratos :wit;
                // 63e18a0 :contratos :subject; 2f4316e :contratos
                // :slot; e70d213 :fonte :tag + :branch) — the typed
                // slot's valid set matches its downstream consumer's
                // accepted set (here, the git porcelain's refname /
                // commit-OID grammars at `git fetch` / `git checkout`
                // time), structurally. Same diagnostic shape every
                // per-axis value-shape lift already exposes
                // (`*Invalid { axis, reason }`); the `value:` field
                // carries the offending refname / OID verbatim so the
                // author can grep their caixa.lisp for the
                // `:tag "<value>"` / `:branch "<value>"` /
                // `:rev "<value>"` literal and fix it in one edit.
                for (pin, value) in [(":tag", tag.as_ref()), (":branch", branch.as_ref())] {
                    if let Some(v) = value
                        && let Err(reason) = crate::render::is_git_ref_name(v)
                    {
                        return Err(DepError::FontePinShape {
                            nome: nome.to_string(),
                            pin: pin.to_string(),
                            value: v.clone(),
                            reason,
                        });
                    }
                }
                if let Some(v) = rev.as_ref()
                    && let Err(reason) = crate::render::is_git_oid(v)
                {
                    return Err(DepError::FontePinShape {
                        nome: nome.to_string(),
                        pin: ":rev".to_string(),
                        value: v.clone(),
                        reason,
                    });
                }
                Ok(())
            }
            Self::Path { caminho } => {
                if caminho.is_empty() {
                    return Err(DepError::FonteCaminhoEmpty {
                        nome: nome.to_string(),
                    });
                }
                Ok(())
            }
        }
    }
}

impl Dep {
    /// Build a minimal registry-sourced dep.
    #[must_use]
    pub fn simple(nome: impl Into<String>, versao: impl Into<String>) -> Self {
        Self {
            nome: nome.into(),
            versao: versao.into(),
            fonte: None,
            opcional: false,
            caracteristicas: Vec::new(),
        }
    }

    /// Build a Git-sourced dep (tag-based).
    #[must_use]
    pub fn git(
        nome: impl Into<String>,
        versao: impl Into<String>,
        repo: impl Into<String>,
        tag: impl Into<String>,
    ) -> Self {
        Self {
            nome: nome.into(),
            versao: versao.into(),
            fonte: Some(DepSource::Git {
                repo: repo.into(),
                tag: Some(tag.into()),
                rev: None,
                branch: None,
            }),
            opcional: false,
            caracteristicas: Vec::new(),
        }
    }

    /// Reject dependency entries whose `:nome` or `:versao` are empty,
    /// whose `:nome` is non-empty but not a valid DNS-1123 label,
    /// or whose `:versao` is non-empty but not a valid Cargo-shaped
    /// semver requirement.
    ///
    /// The author surface for `:deps :versao` (and `:deps-dev :versao`)
    /// is the same Cargo-shaped requirement string `:membros :versao`
    /// (validated at [`crate::AplicacaoSpec::validate`] since 9888b13)
    /// and `:children :versao` (validated at
    /// [`crate::SupervisorSpec::validate`] since b38ff3a) carry — and
    /// the lacre pipeline resolves all three axes through the same
    /// [`crate::parse_requirement`] entry-point. Until 2420c44 landed
    /// `:deps :versao` was the last `:versao` axis untyped past
    /// `Caixa::from_lisp`: a malformed-but-non-empty requirement
    /// (`"^bad-version"`, `"^^0.1"`, the canonical git-tag-shape-
    /// leaking-into-:versao `"v0.1"` typo, the accidental
    /// `"not-a-req"`) silently passed parse and the `semver::Error`
    /// surfaced at lacre-resolve time, far from the source
    /// caixa.lisp, with no field naming which `:deps` entry carried
    /// the typo. The diagnostic [`DepError::VersaoInvalid`] carries
    /// the offending entry's `:nome` + the offending `:versao`
    /// verbatim + the parser's own wording in `reason`, so the
    /// author's grep target is unambiguous.
    ///
    /// The author surface for `:deps :nome` is the same DNS-1123 label
    /// the peer caixa-identifier axes carry — top-level Caixa `:nome`
    /// (validated at [`crate::Caixa::validate_nome`] since 6c992f8),
    /// `:membros :caixa` (validated at
    /// [`crate::AplicacaoSpec::validate_membros`] since 3f9d7a0),
    /// `:children :caixa` (validated at
    /// [`crate::SupervisorSpec::validate`] since 31bfa43). A `:deps
    /// :nome` value flows verbatim through the lacre pipeline as the
    /// target caixa's `:nome` (which the gate at the *target* side now
    /// rejects if non-DNS-1123) and lands as the rendered caixa's
    /// `lareira-<nome>` Helm chart name segment, the per-dep
    /// `LABEL_PROGRAM` label value, and the `caixa-resolver`'s
    /// `~/.cache/caixa/<org>/<nome>` checkout-directory leaf. Until
    /// this gate landed `:deps :nome` was the fourth and last
    /// DNS-1123-shaped caixa-identifier axis still untyped past
    /// `Caixa::from_lisp`: a syntactically wrong dep name (`"Caixa-
    /// Teia"` uppercase — the canonical "I copied the README header"
    /// typo; `"caixa_teia"` underscore — the Go module / Python
    /// identifier leak; `"caixa-teia."` trailing dot — the FQDN
    /// confusion; `"-caixa-teia"` leading hyphen; a 64-byte slug)
    /// silently passed parse and surfaced at lacre-resolve time when
    /// the resolved target caixa's `:nome` failed *its* DNS-1123 gate
    /// — far from the source `:deps` entry, with a diagnostic naming
    /// the *target's* `:nome` rather than the dep entry that referenced
    /// it. Mirroring the 3f9d7a0 / 31bfa43 / 6c992f8 trajectory through
    /// the lifted [`crate::render::is_dns_1123_label`] predicate (the
    /// "before its third occurrence" PRIME DIRECTIVE boundary, THEORY.md
    /// §I.3.5): every `Dep::nome` past validate is DNS-1123-label-shaped,
    /// so every downstream consumer (caixa-resolver's lacre fetch,
    /// caixa-helm's `lareira-<nome>` chart name, the future M4 per-dep
    /// fan-out emitter) reaches for the name knowing the value is
    /// apiserver-valid without re-validating.
    ///
    /// Empty checks fire first (narrower diagnostic), parse last —
    /// same ordering discipline as
    /// [`crate::AplicacaoSpec::validate_membros`] and
    /// [`crate::SupervisorSpec::validate`]. `parse_requirement("")`
    /// returns `Ok(VersionReq::STAR)`, so the empty-`:versao` arm is
    /// structurally necessary even with the parse arm in place. The
    /// `:nome` shape gate runs after the `:nome` empty gate and before
    /// the `:versao` checks so a one-entry caixa.lisp with both wrong
    /// sees the name-side diagnostic first (the name is the
    /// self-locating axis — without it, the parse diagnostic can't
    /// quote `:nome "<bad>"`).
    pub fn validate(&self) -> Result<(), DepError> {
        if self.nome.is_empty() {
            return Err(DepError::NomeEmpty);
        }
        if let Err(reason) = crate::render::is_dns_1123_label(&self.nome) {
            return Err(DepError::NomeInvalid {
                nome: self.nome.clone(),
                reason,
            });
        }
        if self.versao.is_empty() {
            return Err(DepError::VersaoEmpty {
                nome: self.nome.clone(),
            });
        }
        if let Err(e) = crate::parse_requirement(&self.versao) {
            return Err(DepError::VersaoInvalid {
                nome: self.nome.clone(),
                versao: self.versao.clone(),
                reason: e.to_string(),
            });
        }
        if let Some(ref fonte) = self.fonte {
            fonte.validate(&self.nome)?;
        }
        self.validate_caracteristicas()?;
        Ok(())
    }

    /// Reject per-entry `:caracteristicas` (feature-flag) values that
    /// are operationally meaningless. The `:caracteristicas` slot is
    /// a set of feature toggles to enable on the target caixa — same
    /// shape as Cargo's `[dependencies.<dep>.features]` list — and
    /// two structural footguns close here:
    ///
    ///   - empty-string entry (`(:caracteristicas (""))`): the future
    ///     caixa-resolver lacre pipeline would consume the empty
    ///     identifier as a no-op feature enable, silently dropping the
    ///     author's intent far from the source `caixa.lisp`;
    ///   - duplicate entry within one dep (`(:caracteristicas ("http"
    ///     "http"))`): the feature-toggle slot is set-shaped (enabling
    ///     a feature twice has no additional semantic — there is no
    ///     `feature × 2`), so two entries naming the same feature are
    ///     a silent miscount, the same set-not-multiset distinction
    ///     every peer Vec-keyed-by-name axis already closes
    ///     ([`crate::SupervisorError::DuplicateChildCaixa`] on
    ///     `:children :caixa`, [`crate::AplicacaoError::MembroDuplicate`]
    ///     on `:membros :caixa`, [`crate::AplicacaoError::ContratoDuplicate`]
    ///     on `:contratos`, [`crate::AplicacaoError::PlacementClusterDuplicate`]
    ///     on `:placement :clusters`, [`crate::AplicacaoError::EntradaPathDuplicate`]
    ///     on `:entrada :paths`, [`crate::UpgradeError::DuplicateFrom`]
    ///     on `:upgrade-from :from`, [`crate::UpgradeError::DuplicateLoadModule`]
    ///     / [`crate::UpgradeError::DuplicateStateChange`] /
    ///     [`crate::UpgradeError::DuplicateCleanup`] on the within-
    ///     entry `:upgrade-from` axes, and [`DepError::DuplicateNome`]
    ///     on the cross-entry `:deps`/`:deps-dev` `:nome` axis the
    ///     immediate-predecessor 359fba5 closed).
    ///
    /// Same linear-walk + `HashSet` + first-collision diagnostic shape
    /// every peer set-not-multiset gate uses; the empty arm fires
    /// before the duplicate arm so an entry with both an empty feature
    /// *and* a duplicate of some later feature surfaces the empty-
    /// shape diagnostic first (the empty-feature axis is the
    /// more-actionable defect since the missing-name renders the
    /// duplicate-key arm ambiguous: two `""` entries would both report
    /// `caracteristica: ""` with no way to distinguish the offending
    /// site). Empty-first cascade discipline mirrors every peer per-
    /// entry shape + duplicate gate
    /// (`SupervisorSpec::validate`'s `EmptyChildName` before
    /// `DuplicateChildCaixa`; `validate_membros`'s `MembroCaixaEmpty`
    /// before `MembroDuplicate`).
    ///
    /// The per-entry value-shape gate (Cargo-feature-name grammar via
    /// the lifted [`crate::render::is_cargo_feature_name`] predicate)
    /// fires between the empty arm and the duplicate arm — the
    /// canonical per-entry-shape-before-cross-entry-uniqueness
    /// precedence every peer two-arm + value-shape gate establishes
    /// ([`crate::SupervisorSpec::validate`]'s `EmptyChildName` →
    /// `ChildCaixaInvalid` → `DuplicateChildCaixa`,
    /// [`crate::AplicacaoSpec::validate_membros`]'s `MembroCaixaEmpty`
    /// → `MembroCaixaInvalid` → `MembroDuplicate`, [`Dep::validate`]'s
    /// `NomeEmpty` → `NomeInvalid` → cross-list `DuplicateNome`).
    /// Until the value-shape arm landed `:caracteristicas` accepted
    /// every non-empty distinct string — a structurally invalid
    /// feature name (`"http feature"` whitespace, `"+http"` the
    /// canonical paste-from-`+optional-feature` doc activation-form
    /// footgun, `"-flag"` leading hyphen, `".feat"` leading dot,
    /// `"http/json"` Cargo's `dep/feat` namespaced-dep syntax that
    /// only applies inside list-grammar contexts, `"http,json"`
    /// list-separator-belongs-to-the-list-grammar miscomprehension,
    /// `"café"` un-percent-encoded non-ASCII silently round-tripping
    /// inconsistently across NFC/NFD normalization, the 65-byte
    /// paste-from-binary slug) silently passed validate and the
    /// failure surfaced at `cargo metadata` time as the
    /// `restricted_names::validate_feature_name` parser's rejection,
    /// far from the source `caixa.lisp`, with no field naming which
    /// `:deps` entry's `:caracteristicas` carried the typo. The
    /// lifted predicate makes the Cargo-feature-name-grammar
    /// intersection-floor a substrate-level invariant at validate
    /// time — same trajectory as the eight peer
    /// [`crate::render`] value-shape predicates each typed surface
    /// downstream of a structured grammar already follows
    /// ([`is_dns_1123_label`](crate::render::is_dns_1123_label),
    /// [`is_gateway_api_http_path`](crate::render::is_gateway_api_http_path),
    /// [`is_wit_world_ref`](crate::render::is_wit_world_ref),
    /// [`is_nats_subject`](crate::render::is_nats_subject),
    /// [`is_wasi_keyvalue_slot`](crate::render::is_wasi_keyvalue_slot),
    /// [`is_git_ref_name`](crate::render::is_git_ref_name),
    /// [`is_git_oid`](crate::render::is_git_oid),
    /// [`is_git_repo_url`](crate::render::is_git_repo_url)).
    fn validate_caracteristicas(&self) -> Result<(), DepError> {
        let mut seen = std::collections::HashSet::new();
        for c in &self.caracteristicas {
            if c.is_empty() {
                return Err(DepError::CaracteristicaEmpty {
                    nome: self.nome.clone(),
                });
            }
            if let Err(reason) = crate::render::is_cargo_feature_name(c) {
                return Err(DepError::CaracteristicaInvalid {
                    nome: self.nome.clone(),
                    caracteristica: c.clone(),
                    reason,
                });
            }
            if !seen.insert(c.as_str()) {
                return Err(DepError::CaracteristicaDuplicate {
                    nome: self.nome.clone(),
                    caracteristica: c.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Cross-slot coherence gate on the dep-graph axis: no `:deps` or
/// `:deps-dev` entry may name the caixa's own `:nome`.
///
/// A caixa that lists itself as a dep is a degenerate self-edge in the
/// lacre closure's dep-graph — the closure is a DAG rooted at the
/// caixa's `:nome`, and the caixa-resolver's lacre pipeline traverses
/// every `:deps` / `:deps-dev` entry's target by name. A self-dep
/// hands the resolver a node that is its own parent: a one-node cycle
/// it either rejects mid-traversal far from the source `caixa.lisp`
/// (the resolver detecting infinite recursion on the closure walk) or,
/// worse, recurses on until it exhausts its stack. Because every
/// `:nome` is a globally-unique substrate identity (DNS-1123 label +
/// lacre closure root), a dep entry whose `:nome` equals the caixa's
/// own `:nome` *is* the caixa itself, not a coincidentally-named peer.
///
/// Lives outside [`Caixa::validate_deps`] because the dep-list view
/// carries the entries but not the parent `:nome`; mirrors the
/// cross-slot self-edge gates [`crate::supervisor::validate_no_self_supervision`]
/// (ad4abf1) on the `:children :caixa` axis and
/// [`crate::aplicacao::validate_no_self_membership`] on the
/// `:membros :caixa` axis — the same "an edge from a graph node to
/// itself is structurally not a tree/graph edge" discipline, here on
/// the third typed-name-graph axis (the dep closure; the supervision
/// tree and the Aplicacao membership set were the prior two).
///
/// Walks `:deps` first then `:deps-dev` so the diagnostic for a caixa
/// that self-references on both axes surfaces the `:deps` arm first —
/// the load-bearing axis the lacre closure resolves at every build,
/// peer with the canonical [`Caixa::validate_deps`] walk order
/// (`:deps` → `:deps-dev`).
///
/// Carries the offending list tag (`":deps"` or `":deps-dev"`)
/// verbatim into the diagnostic so the author can grep their
/// `caixa.lisp` for the offending block in one edit — same
/// `list: &'static str` shape [`DepError::DuplicateNome`] (359fba5)
/// uses on the cross-list duplicate-name axis.
///
/// `Code paths` (`:bibliotecas` / `:exe` / `:servicos`) are the
/// substrate-blessed shape for referencing the caixa's *own* code, so
/// the diagnostic names them as the corrective surface — every
/// legitimate "I want to use code from this caixa" authoring intent
/// routes through one of those three slots, not a self-dep.
pub fn validate_no_self_dep(
    deps: &[Dep],
    deps_dev: &[Dep],
    parent_nome: &str,
) -> Result<(), DepError> {
    for dep in deps {
        if dep.nome == parent_nome {
            return Err(DepError::DepIsSelf {
                nome: parent_nome.to_string(),
                list: ":deps",
            });
        }
    }
    for dep in deps_dev {
        if dep.nome == parent_nome {
            return Err(DepError::DepIsSelf {
                nome: parent_nome.to_string(),
                list: ":deps-dev",
            });
        }
    }
    Ok(())
}

/// Errors raised by [`Dep::validate`].
///
/// Mirrors the per-axis error families the other `:versao`-carrying
/// typed surfaces expose
/// ([`crate::AplicacaoError::MembroVersaoEmpty`] /
/// [`crate::AplicacaoError::MembroVersaoInvalid`],
/// [`crate::SupervisorError::EmptyChildVersion`] /
/// [`crate::SupervisorError::ChildVersaoInvalid`]) so a future top-
/// level `CaixaError` (M4) sums these without reshaping the diagnostic.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DepError {
    #[error(
        ":deps entry has empty :nome (every dep must name a target caixa; \
         omit the entry instead of carrying an empty name)"
    )]
    NomeEmpty,
    #[error(
        ":deps entry :nome {nome:?} is not a valid DNS-1123 label: {reason} \
         (the value flows verbatim as the target caixa's `:nome`, the rendered \
         `lareira-<nome>` Helm chart name segment, the `LABEL_PROGRAM` label \
         value, and the resolver's checkout-directory leaf — each apiserver-side \
         schema rejects non-DNS-1123 names at admission time; use a lowercase \
         RFC 1123 label like `\"caixa-teia\"` or `\"pleme-mesh\"`, 1..=63 bytes, \
         pattern `^[a-z0-9]([-a-z0-9]*[a-z0-9])?$`)"
    )]
    NomeInvalid { nome: String, reason: String },
    #[error(
        ":deps entry {nome:?} has empty :versao (every dep must pin a semver \
         constraint that resolves through the lacre pipeline)"
    )]
    VersaoEmpty { nome: String },
    #[error(
        ":deps entry {nome:?} :versao {versao:?} is not a valid semver \
         requirement: {reason} (use Cargo-shaped forms like `\"^0.1\"`, \
         `\"~0.1.2\"`, `\"0.1.0\"`, or `\"*\"` — the same shape `:membros :versao` \
         and `:children :versao` carry; the lacre pipeline resolves all three \
         through the same parser)"
    )]
    VersaoInvalid {
        nome: String,
        versao: String,
        reason: String,
    },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo git …) has empty :repo \
         (every git source must name a repo — use a `github:org/repo` \
         shorthand, an `https://…` URL, or an ssh-git URL; omit the \
         entire :fonte block to fall back to the default-host resolver \
         convention)"
    )]
    FonteRepoEmpty { nome: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo git …) :repo {repo:?} has \
         invalid value-shape: {reason} (the value flows verbatim into the \
         caixa-resolver's `git clone <repo>` subprocess invocation; every \
         documented form carries a `:` separator and no whitespace / \
         control / non-ASCII bytes — use a `github:org/repo` shorthand, \
         an `https://host/path` / `ssh://[user@]host/path` / \
         `git://host/path` / `file:///path` URL, or the `git@host:path` \
         scp-style SSH form)"
    )]
    FonteRepoShape {
        nome: String,
        repo: String,
        reason: String,
    },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo git …) has no pin set \
         (set exactly one of :tag, :rev, or :branch so the resolver \
         can pick a reproducible commit; omit the entire :fonte block \
         to fall back to the default-host resolver convention, which \
         resolves the latest tag matching :versao)"
    )]
    FontePinMissing { nome: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo git …) has multiple pins \
         set ({pins}); exactly one of :tag, :rev, or :branch must be \
         set so the resolver's checkout target is unambiguous (the \
         resolver's silent precedence is :rev > :tag > :branch — if \
         you intended one specifically, drop the others)"
    )]
    FontePinAmbiguous { nome: String, pins: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo git …) has empty {pin} \
         (a set pin must name a non-empty git ref; drop the {pin} key \
         entirely to fall through to another pin axis)"
    )]
    FontePinEmpty { nome: String, pin: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo git …) {pin} {value:?} has invalid \
         value-shape: {reason} (the git porcelain enforces the same shape at \
         `git fetch` / `git checkout` time on every pin; use a leaf refname \
         like `\"v0.1.0\"` for `:tag` or `\"main\"` / `\"feature/foo\"` for \
         `:branch`, or a full 40/64 lowercase-hex commit OID for `:rev` — \
         drop any `refs/heads/` or `refs/tags/` prefix the caixa-resolver \
         prepends at clone time, and avoid abbreviated SHAs which are \
         ambiguous across repository history)"
    )]
    FontePinShape {
        nome: String,
        pin: String,
        value: String,
        reason: String,
    },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) has empty :caminho \
         (every path source must name a non-empty filesystem path; \
         omit the entire :fonte block to fall back to the default-host \
         resolver convention)"
    )]
    FonteCaminhoEmpty { nome: String },
    #[error(
        "{list} carries duplicate entry :nome {nome:?} — every dep list keys its \
         entries by caixa name (Cargo's [dependencies] / [dev-dependencies] tables \
         apply the same set-not-multiset discipline; one package per table), and \
         two entries naming the same caixa carry two version constraints / source \
         pins / feature sets for one identity. The caixa-resolver's lacre pipeline \
         consumes the list as a `HashMap`-keyed-by-`:nome` lookup: the second entry \
         silently overwrites the first at the resolver-side `concrete_versao` step, \
         and the dropped entry's pin / features never reach the closure — far from \
         the source caixa.lisp, with no field naming which `:deps` entry was the \
         silent loser. If two version constraints are genuinely needed (the rare \
         multi-version closure case the lacre pipeline doesn't yet support), the \
         author surface is two distinct caixa names (e.g. a `caixa-teia-v01` / \
         `caixa-teia-v02` aliased pair); within one list, one entry per caixa name."
    )]
    DuplicateNome { nome: String, list: &'static str },
    #[error(
        ":deps entry {nome:?} has empty :caracteristicas entry — every feature flag must \
         name a non-empty identifier on the target caixa (Cargo's [dependencies.<dep>.features] \
         applies the same per-entry non-empty discipline). An empty feature flag reaches the \
         caixa-resolver's lacre pipeline as a no-op feature enable, silently dropping the \
         author's intent far from the source caixa.lisp; drop the empty entry, or replace it \
         with the canonical kebab-case feature name the target caixa declares."
    )]
    CaracteristicaEmpty { nome: String },
    #[error(
        ":deps entry {nome:?} :caracteristicas entry {caracteristica:?} is not a valid Cargo \
         feature name: {reason} (the value flows verbatim into Cargo's \
         [dependencies.<dep>.features] list and Cargo's `restricted_names::validate_feature_name` \
         parser enforces the same shape at `cargo metadata` time; use a single-token \
         identifier like `\"http\"`, `\"derive\"`, or `\"runtime-tokio\"` — kebab-case ASCII \
         alphanumeric with `-`, `_`, `+`, or `.` as continuation characters, starting with \
         an ASCII alphanumeric or `_`)"
    )]
    CaracteristicaInvalid {
        nome: String,
        caracteristica: String,
        reason: String,
    },
    #[error(
        ":deps entry {nome:?} :caracteristicas carries duplicate feature {caracteristica:?} — \
         every feature-flag list keys its entries by name (Cargo's \
         [dependencies.<dep>.features] applies the same set-not-multiset discipline; one entry \
         per feature per dep), and two entries naming the same feature are a redundant \
         set-membership declaration for one identity (the feature-toggle slot is set-shaped; \
         enabling a feature twice has no additional semantic). The caixa-resolver's lacre \
         pipeline consumes the list as a set-shaped feature toggle — the resolver enables the \
         feature once regardless of declaration count, so the duplicate's pin / position never \
         reaches the closure with no field naming the silent loser. One entry per feature per \
         dep; if two distinct features are intended, name each verbatim."
    )]
    CaracteristicaDuplicate {
        nome: String,
        caracteristica: String,
    },
    #[error(
        "{list} entry :nome {nome:?} names the caixa itself — a caixa cannot depend \
         on itself (the lacre closure's dep-graph traversal is rooted at the caixa's \
         :nome, and a self-dep would be a one-node cycle the caixa-resolver either \
         rejects mid-traversal far from the source caixa.lisp or recurses on until \
         it exhausts its stack). Every :nome is globally-unique substrate identity, \
         so a :deps / :deps-dev entry whose :nome equals the parent caixa's :nome \
         *is* the parent itself, not a coincidentally-named peer. Drop the \
         self-referential dep entry — to reference code from this caixa, use \
         :bibliotecas / :exe / :servicos (the substrate-blessed shape for \
         referencing the caixa's own code surface) instead."
    )]
    DepIsSelf { nome: String, list: &'static str },
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_dep_is_minimal() {
        let d = Dep::simple("caixa-teia", "^0.1");
        assert_eq!(d.nome, "caixa-teia");
        assert_eq!(d.versao, "^0.1");
        assert!(d.fonte.is_none());
        assert!(!d.opcional);
        assert!(d.caracteristicas.is_empty());
    }

    #[test]
    fn git_dep_carries_tag() {
        let d = Dep::git("t", "*", "github:o/r", "v1");
        match d.fonte {
            Some(DepSource::Git {
                ref repo, ref tag, ..
            }) => {
                assert_eq!(repo, "github:o/r");
                assert_eq!(tag.as_deref(), Some("v1"));
            }
            _ => panic!("expected Git source"),
        }
    }

    #[test]
    fn validate_accepts_simple_dep() {
        Dep::simple("caixa-teia", "^0.1").validate().unwrap();
    }

    #[test]
    fn validate_rejects_empty_nome() {
        // The fail-before-pass-after pin for `:nome ""`: the empty-name
        // arm fires first so the per-entry parse-side diagnostic doesn't
        // emit a useless `nome: ""` reference.
        let mut d = Dep::simple("placeholder", "^0.1");
        d.nome = String::new();
        assert_eq!(d.validate().unwrap_err(), DepError::NomeEmpty);
    }

    #[test]
    fn validate_rejects_empty_versao() {
        // `parse_requirement("")` returns `Ok(VersionReq::STAR)` (the
        // semver crate accepts the empty string as a wildcard match),
        // so the empty-`:versao` arm is structurally necessary even
        // with the parse arm in place — mirrors `MembroVersaoEmpty` /
        // `EmptyChildVersion` ordering on the other two `:versao` axes.
        let mut d = Dep::simple("caixa-teia", "ignored");
        d.versao = String::new();
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::VersaoEmpty { ref nome } if nome == "caixa-teia"),
            "got {err:?}"
        );
    }

    // ── value-shape: DNS-1123 label on :deps :nome ────────────────────────

    #[test]
    fn validate_rejects_nome_with_uppercase() {
        // The fail-before-pass-after pin: a non-empty but uppercase
        // `:nome` silently passed `validate()` on every pre-gate
        // codebase because the prior shape only refused the empty
        // string. The DNS-1123 violation surfaced far downstream at
        // lacre-resolve time when the *target* caixa's `:nome` failed
        // its own gate — far from the `:deps` entry, with a diagnostic
        // naming the target rather than the dep entry that referenced
        // it. Same fail-before-pass-after fixture pinned for
        // `:membros :caixa` (3f9d7a0), `:children :caixa` (31bfa43),
        // and Caixa `:nome` (6c992f8).
        let d = Dep::simple("Caixa-Teia", "^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::NomeInvalid { ref nome, ref reason }
                    if nome == "Caixa-Teia" && reason.contains("uppercase")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_nome_with_underscore() {
        // RFC 1123 allows `[a-z0-9-]` only; underscore is the canonical
        // "I'm thinking of Go module names / Python identifiers" leak.
        // Same fixture pinned for the peer caixa-identifier axes.
        let d = Dep::simple("caixa_teia", "^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::NomeInvalid { ref nome, ref reason }
                    if nome == "caixa_teia" && reason.contains('_')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_nome_with_dot() {
        // A `:deps :nome` is a single DNS-1123 *label*, not a
        // subdomain — dots are rejected. The `"caixa.teia"` shape is
        // the canonical "I confused the dep name with the FQDN /
        // namespace" footgun, distinct from the legitimate
        // `:fonte :repo "github:org/caixa-teia"` axis.
        let d = Dep::simple("caixa.teia", "^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::NomeInvalid { ref nome, ref reason }
                    if nome == "caixa.teia" && reason.contains('.')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_nome_with_leading_hyphen() {
        // RFC 1123 requires alphanumeric at both label boundaries.
        // Pinned in parity with the peer DNS-1123 fixtures.
        let d = Dep::simple("-caixa-teia", "^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::NomeInvalid { ref nome, ref reason }
                    if nome == "-caixa-teia" && reason.contains("alphanumeric")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_nome_with_trailing_hyphen() {
        let d = Dep::simple("caixa-teia-", "^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::NomeInvalid { ref nome, ref reason }
                    if nome == "caixa-teia-" && reason.contains("alphanumeric")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_nome_with_slash() {
        // The canonical "I copied the GitHub repo path into `:nome`
        // instead of `:fonte :repo`" typo. A `/` in the name leaks the
        // lacre-side `:repositorio` shape (`pleme-io/caixa-teia`) into
        // the local-name slot. Same fixture pinned for `:membros
        // :caixa` (3f9d7a0).
        let d = Dep::simple("pleme-io/caixa-teia", "^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::NomeInvalid { ref nome, ref reason }
                    if nome == "pleme-io/caixa-teia" && reason.contains('/')
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_nome_too_long() {
        // 64-byte label — one over the RFC 1035 / RFC 1123 label cap.
        // Built from a valid character set so the length-bound
        // diagnostic surfaces before any per-character check (the
        // order pin parallel to the per-character predicates inside
        // [`crate::render::is_dns_1123_label`]).
        let long = "a".repeat(64);
        let d = Dep::simple(&long, "^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::NomeInvalid { ref nome, ref reason }
                    if nome.len() == 64 && reason.contains("max length of 63")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_canonical_nome_labels() {
        // Positive-control sweep — every form the K8s apiserver
        // accepts as a DNS-1123 label must round-trip through
        // validate. Covers a hyphen-bearing label, a numeric-suffix
        // label, a leading-digit label, a single-character label, and
        // a 63-byte (exactly the cap) label — the same fixture set
        // the peer `:membros :caixa` / `:children :caixa` positive
        // controls pin.
        for nome in [
            "caixa-teia",
            "caixa-resolver2",
            "2nd-tier-cache",
            "x",
            "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0",
        ] {
            Dep::simple(nome, "^0.1")
                .validate()
                .unwrap_or_else(|e| panic!("canonical label {nome:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn nome_empty_takes_precedence_over_nome_invalid() {
        // Ordering pin: `NomeEmpty` is the more self-locating
        // diagnostic on `""` and must lead — `is_dns_1123_label` is
        // only reached after the empty-check fires at the call site.
        // Mirrors `membro_caixa_empty_takes_precedence_over_invalid`
        // (3f9d7a0) on the peer caixa-identifier axis.
        let mut d = Dep::simple("placeholder", "^0.1");
        d.nome = String::new();
        assert_eq!(d.validate().unwrap_err(), DepError::NomeEmpty);
    }

    #[test]
    fn nome_invalid_fires_before_versao_empty() {
        // Ordering pin: a malformed `:nome` fires before any `:versao`
        // axis check on the *same* entry — the per-entry shape gates
        // run top-to-bottom (nome empty → nome shape → versao empty →
        // versao parse → fonte shape), so a one-entry caixa.lisp with
        // both wrong sees the name-side diagnostic first (the name is
        // the self-locating axis — without a valid name, the parse
        // diagnostic can't quote `:nome "<bad>"`). Same ordering
        // discipline as `membro_caixa_invalid_fires_before_versao_check`
        // (3f9d7a0).
        let mut d = Dep::simple("Caixa-Teia", "^0.1");
        d.versao = String::new();
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::NomeInvalid { ref nome, .. } if nome == "Caixa-Teia"),
            "got {err:?}"
        );
    }

    #[test]
    fn nome_invalid_fires_before_versao_invalid() {
        // Ordering pin: a malformed `:nome` fires before the `:versao`
        // parse-side check on the *same* entry. Pin separately from
        // the empty-versao ordering so a future re-ordering surfaces
        // here, parallel to the b0c8389 / c4213a4 trajectory.
        let d = Dep::simple("Caixa-Teia", "^^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::NomeInvalid { ref nome, .. } if nome == "Caixa-Teia"),
            "got {err:?}"
        );
    }

    #[test]
    fn nome_invalid_fires_before_fonte_invalid() {
        // Ordering pin: a malformed `:nome` fires before the `:fonte`
        // shape check on the *same* entry. The `:fonte` diagnostic
        // names the offending dep's `:nome` verbatim (via
        // `DepSource::validate(&self.nome)`), so a non-self-locating
        // name would taint the downstream diagnostic too — the gate
        // ordering keeps both diagnostics individually self-locating.
        let mut d = Dep::simple("Caixa-Teia", "^0.1");
        d.fonte = Some(DepSource::Git {
            repo: String::new(),
            tag: None,
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::NomeInvalid { ref nome, .. } if nome == "Caixa-Teia"),
            "got {err:?}"
        );
    }

    #[test]
    fn nome_invalid_diagnostic_carries_offending_name() {
        // The diagnostic-shape pin: the error names the offending
        // `:nome` value verbatim so the author can grep their
        // caixa.lisp without re-running the build, and carries a
        // non-empty `reason` from `is_dns_1123_label` so the
        // predicate's own wording flows through to the diagnostic.
        // Same shape as `MembroCaixaInvalid` (3f9d7a0),
        // `ChildCaixaInvalid` (31bfa43), `ManifestError::NomeInvalid`
        // (6c992f8) — the four DNS-1123 caixa-identifier axes now
        // share a structurally-equivalent diagnostic family.
        let d = Dep::simple("Caixa_Teia", "^0.1");
        let err = d.validate().unwrap_err();
        let DepError::NomeInvalid { nome, reason } = err else {
            panic!("expected NomeInvalid, got other variant");
        };
        assert_eq!(nome, "Caixa_Teia");
        assert!(
            !reason.is_empty(),
            "NomeInvalid `reason` must carry the predicate's wording verbatim"
        );
    }

    #[test]
    fn validate_rejects_invalid_versao_requirement() {
        // The fail-before-pass-after pin: a non-empty but malformed
        // requirement (`"^bad-version"`) silently passed every pre-gate
        // codebase because `:deps :versao` wasn't validated. The parse
        // failure surfaced far downstream at lacre-resolve time with a
        // `semver::Error` that didn't name which `:deps` entry carried
        // the typo. The new gate moves the check to caixa-build time
        // at the source caixa.lisp.
        let d = Dep::simple("caixa-teia", "^bad-version");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::VersaoInvalid { ref nome, ref versao, .. }
                    if nome == "caixa-teia" && versao == "^bad-version"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_versao_with_double_caret_typo() {
        // `"^^0.1"` is the canonical doubled-caret typo — looks like a
        // Cargo-shaped requirement on first glance but fails the parser
        // because semver doesn't accept stacked operators. Pin this
        // adjacent-shape footgun explicitly so a future relaxation that
        // accepts "looks-canonical-but-isn't" forms surfaces here, in
        // parity with the `:membros` / `:children` fixtures.
        let d = Dep::simple("caixa-teia", "^^0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::VersaoInvalid { ref nome, ref versao, .. }
                    if nome == "caixa-teia" && versao == "^^0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_versao_with_v_prefixed_tag() {
        // `"v0.1"` is the canonical "git-tag-shape leaking into the
        // semver requirement slot" typo — an author copies the
        // publish-side git-tag string verbatim into `:versao`, but
        // Cargo's semver parser rejects the leading `v`. Same fixture
        // pinned for `:membros :versao` (9888b13) and `:children
        // :versao` (b38ff3a). (Bare `x`-glob shorthands like `^0.1.x`
        // are *accepted* by the semver crate as an `*` wildcard on the
        // patch axis — they're a Cargo-side valid shape, not a typo.)
        let d = Dep::simple("caixa-teia", "v0.1");
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::VersaoInvalid { ref nome, ref versao, .. }
                    if nome == "caixa-teia" && versao == "v0.1"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_canonical_versao_forms() {
        // The five Cargo-shaped requirement forms `:membros :versao`
        // and `:children :versao` already accept via
        // `crate::parse_requirement` must pass the deps gate without
        // re-validating at the resolver layer. Pin every leg so a
        // future tightening of the canonical set surfaces here as a
        // test failure.
        for form in [
            "^0.1",      // caret — minor-range pin (the most common shape)
            "~0.1.2",    // tilde — patch-range pin
            "0.1.0",     // exact — single-version pin
            "*",         // wildcard — explicitly any-version
            ">=0.1, <2", // multi-range — comma-separated comparators
        ] {
            Dep::simple("caixa-teia", form)
                .validate()
                .unwrap_or_else(|e| panic!("canonical form {form:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn versao_empty_takes_precedence_over_invalid() {
        // Order pin: the existing `VersaoEmpty` diagnostic (which
        // doesn't try to parse) fires before the new `VersaoInvalid`
        // parse-side diagnostic, so an empty `:versao` keeps its
        // narrower error message — `parse_requirement("")` would
        // otherwise return `Ok(STAR)` and silently pass, but the empty
        // arm catches it first.
        let mut d = Dep::simple("caixa-teia", "ignored");
        d.versao = String::new();
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::VersaoEmpty { ref nome } if nome == "caixa-teia"),
            "got {err:?}"
        );
    }

    #[test]
    fn nome_empty_takes_precedence_over_versao_invalid() {
        // Order pin: even when `:versao` is malformed and would raise
        // its own diagnostic, `:nome ""` fires first because the
        // per-entry parse diagnostic needs a non-empty name to be
        // self-locating. Mirrors the
        // `membros_validation_runs_before_contratos_membership_check`
        // ordering on the typed-graph layer.
        let mut d = Dep::simple("placeholder", "^bad");
        d.nome = String::new();
        let err = d.validate().unwrap_err();
        assert_eq!(err, DepError::NomeEmpty);
    }

    #[test]
    fn versao_invalid_diagnostic_carries_offending_versao() {
        // The diagnostic-shape pin: the error names the offending
        // `:versao` value verbatim so the author can grep their
        // caixa.lisp without re-running the build, and carries a
        // non-empty `reason` from `semver::VersionReq::parse` so the
        // parser's own wording flows through to the diagnostic.
        let d = Dep::simple("caixa-teia", "not-a-req");
        let err = d.validate().unwrap_err();
        let DepError::VersaoInvalid {
            nome,
            versao,
            reason,
        } = err
        else {
            panic!("expected VersaoInvalid, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(versao, "not-a-req");
        assert!(
            !reason.is_empty(),
            "VersaoInvalid `reason` must carry the parser's wording verbatim"
        );
    }

    // -- :fonte value-shape gate ------------------------------------------

    fn dep_with_fonte(fonte: DepSource) -> Dep {
        let mut d = Dep::simple("caixa-teia", "^0.1");
        d.fonte = Some(fonte);
        d
    }

    #[test]
    fn validate_accepts_git_fonte_with_tag() {
        // The positive-control pin on the canonical git source — exactly
        // one of :tag/:rev/:branch set, non-empty :repo. Mirrors the
        // shape every existing caixa-resolver integration test uses.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_accepts_git_fonte_with_rev() {
        // Each of the three pin axes is independently a valid single-pin
        // shape; pin the :rev arm so a future relaxation that only
        // accepts :tag surfaces here. The value is a full 40-hex SHA-1
        // OID — the canonical `git rev-parse HEAD` emission shape the
        // `crate::render::is_git_oid` value-shape gate now requires;
        // abbreviated OIDs are ambiguous across repo history and
        // rejected at this gate (pinned separately by
        // `validate_rejects_git_fonte_with_rev_abbreviated_prefix`).
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("c0ffee0123abcdef0123456789abcdef01234567".into()),
            branch: None,
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_accepts_git_fonte_with_branch() {
        // The :branch arm is the third valid single-pin shape — pinned
        // separately so the gate-accepts-all-three-pin-axes contract is
        // a build-error to relax.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: None,
            branch: Some("main".into()),
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_accepts_path_fonte() {
        // The positive-control pin on the path source — non-empty
        // :caminho, no pin axes (paths have no commit identity). Pinned
        // so a future "paths must also pin a rev" tightening surfaces
        // here as a structural decision, not a silent break.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_rejects_git_fonte_with_empty_repo() {
        // The fail-before-pass-after pin for `(:tipo git :repo "" :tag
        // "v1")`: the empty-repo shape silently passed every pre-gate
        // codebase because `:fonte` wasn't validated. The git-clone
        // failure surfaced far downstream at lacre-resolve time with no
        // field naming which `:deps` entry carried the typo. The new
        // gate moves the check to caixa-build time at the source
        // caixa.lisp.
        let d = dep_with_fonte(DepSource::Git {
            repo: String::new(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteRepoEmpty { ref nome } if nome == "caixa-teia"),
            "got {err:?}"
        );
    }

    // -- :repo value-shape gate -------------------------------------------
    //
    // The `:fonte (:tipo git :repo …)` value flows verbatim into the
    // caixa-resolver's `git clone <repo>` subprocess. The pre-gate
    // codebase admitted any non-empty string; the new
    // [`crate::render::is_git_repo_url`] predicate gates the git-porcelain
    // URL intersection-floor at validate time, peer with the three pin
    // axes (`:tag` + `:branch` via `is_git_ref_name`, `:rev` via
    // `is_git_oid`). Every test in this section is a fail-before /
    // pass-after pin on a specific authoring footgun.

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_trailing_space() {
        // The canonical paste-from-doc footgun on `:repo` — an author
        // copies `"github:pleme-io/caixa-teia "` (trailing space) out of
        // a doc paragraph. Until this gate landed the empty-repo arm
        // passed (the string isn't empty), the resolver issued
        // `git clone 'github:pleme-io/caixa-teia '`, and the failure
        // surfaced at clone time with a quoting-confused error far from
        // the source caixa.lisp. Same paste-from-doc footgun the
        // `:tag "v0.1.0 "` gate (e70d213) closes on the peer refname
        // axis — now closed on the `:repo` URL axis too.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia ".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "github:pleme-io/caixa-teia ");
        assert!(
            reason.contains("whitespace"),
            "reason must surface the whitespace arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_starting_with_dash() {
        // The canonical CLI-argument-injection footgun at the `git clone`
        // subprocess boundary — `:repo "-upload-pack=evil"` makes git's
        // argv parser read the value as a CLI flag, escaping the
        // subprocess argument boundary. The `--` separator workaround
        // does not fix the typed slot's accepted set; the gate rejects
        // the shape upstream at validate time so the resolver never
        // invokes a `git clone -…` subprocess.
        let d = dep_with_fonte(DepSource::Git {
            repo: "-upload-pack=evil".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { repo, reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(repo, "-upload-pack=evil");
        assert!(
            reason.contains("must not start with `-`"),
            "reason must surface the leading-`-` arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_embedded_newline() {
        // The canonical paste-from-multiline-doc footgun — a `:repo`
        // string with an embedded `\n` silently breaks git's URL parser
        // and is a class of CRLF-injection at the subprocess-argument
        // boundary. Caught by the control-char arm (0x0A < 0x20).
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia\nrm -rf /".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("control character"),
            "reason must surface the control-char arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_tab() {
        // Tab is the sibling whitespace footgun (the canonical
        // copy-from-aligned-table paste); pinned separately from the
        // space arm so a future relaxation that only catches one
        // surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia\t".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteRepoShape { ref reason, .. }
                    if reason.contains("whitespace")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_non_ascii() {
        // IDN hosts must be pre-encoded as Punycode (`xn--…`) — raw
        // non-ASCII silently breaks at git's URL parser and round-trips
        // inconsistently across NFC/NFD normalization on APFS /
        // case-folding filesystems. Same intersection-floor
        // [`is_git_ref_name`] enforces on the refname axes.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/café".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteRepoShape { ref reason, .. }
                    if reason.contains("non-ASCII")
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_missing_colon_separator() {
        // The "I dropped the scheme" footgun — `:repo "pleme-io/caixa-teia"`
        // (no `github:` prefix, no scheme). Every documented form
        // carries a `:` (`github:`, `https://`, `ssh://`, `git://`,
        // `file://`, or `git@host:path`); a bare `org/repo` is
        // ambiguous (`git clone` reads as a relative filesystem path
        // rather than the GitHub-shorthand expansion the author
        // probably intended) and the gate rejects the shape upstream.
        let d = dep_with_fonte(DepSource::Git {
            repo: "pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must contain a `:`"),
            "reason must surface the missing-`:` arm, got {reason:?}"
        );
        assert!(
            reason.contains("github:"),
            "reason must name the canonical `github:` shorthand prefix, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_leading_colon() {
        // The "empty scheme" footgun — `:repo ":foo"` has a zero-length
        // scheme that no git porcelain entry-point accepts. Pinned
        // separately from the missing-`:` arm because a value with a
        // leading `:` does technically contain a `:` separator; the
        // shape gate rejects on a dedicated arm so the diagnostic
        // names the specific footgun.
        let d = dep_with_fonte(DepSource::Git {
            repo: ":pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not start with `:`"),
            "reason must surface the leading-`:` arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_too_long() {
        // The cap arm — a `:repo` value longer than
        // [`crate::render::GIT_REPO_URL_MAX_LEN`] (2048) bytes is
        // structurally untenable on every realistic landing site (the
        // resolver's `git clone` invocation, the future M4 CR
        // materializer's per-dep `repo:` axis); a value of that length
        // is almost certainly a paste-from-binary slug.
        let too_long = format!(
            "github:pleme-io/{}",
            "x".repeat(crate::render::GIT_REPO_URL_MAX_LEN)
        );
        let d = dep_with_fonte(DepSource::Git {
            repo: too_long.clone(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("2048"),
            "reason must name the cap, got {reason:?}"
        );
    }

    #[test]
    fn validate_accepts_canonical_git_fonte_repo_shapes() {
        // The positive-control sweep: every documented author shape on
        // the `:fonte :repo` axis ([`crate::DepSource::Git`] doc comment)
        // must pass the value-shape gate. Pinned so a future tightening
        // (e.g. forbidding `http://` in favor of `https://`-only) surfaces
        // here as a structural decision. Each form is exercised with the
        // same canonical `:tag` pin so only the `:repo` axis varies.
        for repo in [
            // The pleme-io registry-shorthand convention — `github:org/repo`.
            "github:pleme-io/caixa-teia",
            // Other host-aliased shorthands (the resolver's pluggable
            // host-prefix table).
            "gitlab:pleme-io/caixa-teia",
            "codeberg:pleme-io/caixa-teia",
            "sourcehut:~pleme-io/caixa-teia",
            // Full HTTPS URL with and without `.git` suffix.
            "https://github.com/pleme-io/caixa-teia",
            "https://github.com/pleme-io/caixa-teia.git",
            // HTTP (rare; dev / mirror).
            "http://example.com/pleme-io/caixa-teia.git",
            // SSH URL.
            "ssh://git@github.com/pleme-io/caixa-teia.git",
            "ssh://git@git.example.com:2222/pleme-io/caixa-teia.git",
            // Scp-style SSH — the canonical `git@host:path` short form.
            "git@github.com:pleme-io/caixa-teia.git",
            "git@git.example.com:team/private.git",
            // Anonymous git protocol.
            "git://git.example.com/pleme-io/caixa-teia.git",
            // Local file URL (dev path).
            "file:///tmp/caixa-teia",
        ] {
            let d = dep_with_fonte(DepSource::Git {
                repo: repo.into(),
                tag: Some("v0.1.0".into()),
                rev: None,
                branch: None,
            });
            d.validate()
                .unwrap_or_else(|e| panic!("canonical repo {repo:?} must validate, got {e:?}"));
        }
    }

    #[test]
    fn fonte_repo_empty_takes_precedence_over_shape() {
        // Order pin: the existing `FonteRepoEmpty` diagnostic (narrower
        // diagnostic; doesn't try to parse the URL shape) fires before
        // the new `FonteRepoShape` per-axis gate, so an empty `:repo`
        // keeps its narrower error message. Mirrors
        // `fonte_repo_empty_fires_before_pin_missing` (already pinned)
        // on the ordering layer.
        let d = dep_with_fonte(DepSource::Git {
            repo: String::new(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteRepoEmpty { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn fonte_repo_shape_fires_before_pin_missing() {
        // Order pin: a malformed `:repo` value on a dep with no pin set
        // surfaces the `:repo` shape diagnostic (the more self-locating
        // axis — the `:repo` is the load-bearing identity of the source;
        // a missing pin is downstream from "do we even know the repo")
        // rather than collapsing onto the pin-missing diagnostic. The
        // shape gate runs inline before the pin enumeration in
        // `DepSource::validate`.
        let d = dep_with_fonte(DepSource::Git {
            repo: "pleme-io/caixa-teia".into(), // missing `:` separator
            tag: None,
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteRepoShape { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn fonte_repo_shape_diagnostic_carries_offending_repo_verbatim() {
        // The diagnostic-shape pin: the error names the offending
        // `:repo` value verbatim plus a non-empty parser-shaped `reason`
        // so the author can grep their caixa.lisp without re-running
        // the build. Mirrors the diagnostic-shape sweep on every prior
        // value-shape gate (3f9d7a0, 6cbb900, c7d05ec, e70d213, be07fd5).
        let d = dep_with_fonte(DepSource::Git {
            repo: "pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "pleme-io/caixa-teia");
        assert!(
            !reason.is_empty(),
            "FonteRepoShape `reason` must carry the predicate's wording verbatim"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_no_pin() {
        // The fail-before-pass-after pin for the canonical
        // `(:tipo git :repo "github:pleme-io/x")` shape with no
        // :tag/:rev/:branch — until this gate landed the resolver's
        // ResolveError::MissingPin surfaced at fetch time, far from the
        // source caixa.lisp. The new gate moves the check to validate
        // time and names the offending dep.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FontePinMissing { ref nome } if nome == "caixa-teia"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_ambiguous_tag_and_branch() {
        // The canonical "pin drift" footgun: an author writes
        // `:tag "v1"` and later adds `:branch "main"` without removing
        // the :tag, and the resolver silently picks :tag (precedence
        // :rev > :tag > :branch). The :branch was dropped with no
        // diagnostic. The gate now rejects multi-pin shapes so the
        // author makes the precedence explicit at the source.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: Some("main".into()),
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinAmbiguous { nome, pins } = err else {
            panic!("expected FontePinAmbiguous");
        };
        assert_eq!(nome, "caixa-teia");
        assert!(pins.contains(":tag"));
        assert!(pins.contains(":branch"));
        assert!(!pins.contains(":rev"));
    }

    #[test]
    fn validate_rejects_git_fonte_with_ambiguous_tag_and_rev() {
        // Sibling arm of the pin-drift footgun: :tag + :rev set
        // simultaneously. Pinned separately so a future relaxation
        // that only catches the (:tag, :branch) pair surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: Some("c0ffee".into()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinAmbiguous { nome, pins } = err else {
            panic!("expected FontePinAmbiguous");
        };
        assert_eq!(nome, "caixa-teia");
        assert!(pins.contains(":tag"));
        assert!(pins.contains(":rev"));
    }

    #[test]
    fn validate_rejects_git_fonte_with_all_three_pins() {
        // The maximal ambiguity case — every pin axis set. Pinned so a
        // future relaxation that only catches pairs surfaces here. The
        // diagnostic must enumerate every offending axis so the author
        // sees the full set, not just the first match.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: Some("c0ffee".into()),
            branch: Some("main".into()),
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinAmbiguous { nome, pins } = err else {
            panic!("expected FontePinAmbiguous");
        };
        assert_eq!(nome, "caixa-teia");
        assert!(pins.contains(":tag"));
        assert!(pins.contains(":rev"));
        assert!(pins.contains(":branch"));
    }

    #[test]
    fn validate_rejects_git_fonte_with_empty_tag_pin() {
        // The empty-pin arm: exactly one pin axis is `Some(_)`, but its
        // inner string is empty. Distinct from FontePinMissing (where
        // every axis is None) — pinned separately so a future
        // tightening collapsing them surfaces here as a structural
        // decision.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some(String::new()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinEmpty { nome, pin } = err else {
            panic!("expected FontePinEmpty");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(pin, ":tag");
    }

    #[test]
    fn validate_rejects_git_fonte_with_empty_rev_pin() {
        // Sibling arm — the empty-pin diagnostic names which axis
        // carries the empty value, so the author's grep target is
        // unambiguous.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some(String::new()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinEmpty { nome, pin } = err else {
            panic!("expected FontePinEmpty");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(pin, ":rev");
    }

    #[test]
    fn validate_rejects_path_fonte_with_empty_caminho() {
        // The fail-before-pass-after pin for `(:tipo path :caminho "")`:
        // until this gate landed the resolver's
        // ResolveError::MissingPath surfaced with `path: PathBuf("")` at
        // fetch time — not actionable. The new gate moves the check to
        // validate time and names the offending dep.
        let d = dep_with_fonte(DepSource::Path {
            caminho: String::new(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoEmpty { ref nome } if nome == "caixa-teia"),
            "got {err:?}"
        );
    }

    #[test]
    fn fonte_repo_empty_fires_before_pin_missing() {
        // Order pin: empty `:repo` is the more self-locating diagnostic
        // (every git source needs a repo; the pin discussion is
        // secondary), so it fires before the pin-missing arm even when
        // both are violated. Mirrors the
        // `nome_empty_takes_precedence_over_versao_invalid` ordering
        // discipline on the per-entry layer.
        let d = dep_with_fonte(DepSource::Git {
            repo: String::new(),
            tag: None,
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteRepoEmpty { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn fonte_pin_missing_fires_before_pin_empty() {
        // Order pin: a fully-None pin set is structurally distinct from
        // a Some(empty) pin — the first surfaces as FontePinMissing
        // (no axis chosen), the second as FontePinEmpty (axis chosen
        // but value blank). Pin the disjoint relationship so a future
        // unification collapses to one variant only as a structural
        // decision.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: None,
            branch: None,
        });
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::FontePinMissing { .. }
        ));
    }

    #[test]
    fn nome_empty_takes_precedence_over_fonte_invalid() {
        // Order pin: a per-entry diagnostic without a non-empty :nome
        // can't be self-locating, so :nome "" fires first even when
        // :fonte is also malformed. Mirrors
        // `nome_empty_takes_precedence_over_versao_invalid` on the
        // adjacent axis.
        let mut d = dep_with_fonte(DepSource::Git {
            repo: String::new(),
            tag: None,
            rev: None,
            branch: None,
        });
        d.nome = String::new();
        assert_eq!(d.validate().unwrap_err(), DepError::NomeEmpty);
    }

    #[test]
    fn versao_invalid_takes_precedence_over_fonte_invalid() {
        // Order pin: the :versao parse-side diagnostic is narrower than
        // the :fonte shape diagnostic — a malformed :versao always names
        // the parser's reason, which is more actionable than the
        // :fonte gate's "the pins are wrong" wording. Pin the ordering
        // so a re-ordering surfaces here.
        let mut d = dep_with_fonte(DepSource::Git {
            repo: String::new(),
            tag: None,
            rev: None,
            branch: None,
        });
        d.versao = "v0.1".into();
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::VersaoInvalid { ref nome, .. } if nome == "caixa-teia"),
            "got {err:?}"
        );
    }

    #[test]
    fn fonte_invalid_diagnostic_carries_offending_nome() {
        // The diagnostic-shape pin: every :fonte error variant names
        // the offending dep's :nome verbatim, so the author can grep
        // caixa.lisp for the `:nome "<n>"` block and fix it in one
        // edit. Cover all six variants so a future variant addition
        // forces a parallel diagnostic-shape decision.
        for (case, fonte) in [
            (
                "repo-empty",
                DepSource::Git {
                    repo: String::new(),
                    tag: Some("v1".into()),
                    rev: None,
                    branch: None,
                },
            ),
            (
                "repo-shape",
                DepSource::Git {
                    repo: "github:p/x ".into(),
                    tag: Some("v1".into()),
                    rev: None,
                    branch: None,
                },
            ),
            (
                "pin-missing",
                DepSource::Git {
                    repo: "github:p/x".into(),
                    tag: None,
                    rev: None,
                    branch: None,
                },
            ),
            (
                "pin-ambiguous",
                DepSource::Git {
                    repo: "github:p/x".into(),
                    tag: Some("v1".into()),
                    rev: None,
                    branch: Some("main".into()),
                },
            ),
            (
                "pin-empty",
                DepSource::Git {
                    repo: "github:p/x".into(),
                    tag: Some(String::new()),
                    rev: None,
                    branch: None,
                },
            ),
            (
                "caminho-empty",
                DepSource::Path {
                    caminho: String::new(),
                },
            ),
        ] {
            let d = dep_with_fonte(fonte);
            let msg = d
                .validate()
                .expect_err(&format!("{case}: expected fonte error"))
                .to_string();
            assert!(
                msg.contains("\"caixa-teia\""),
                "{case}: diagnostic must quote the offending :nome verbatim, got {msg:?}"
            );
        }
    }

    // -- :tag / :branch value-shape gate ----------------------------------

    #[test]
    fn validate_rejects_git_fonte_with_tag_carrying_trailing_space() {
        // The canonical paste-from-doc footgun on `:tag` — author
        // copies `"v0.1.0 "` (trailing space) out of a release-notes
        // paragraph. Until this gate landed the empty-pin arm passed
        // (the string isn't empty), the resolver issued
        // `git fetch <remote> tag 'v0.1.0 '`, and the failure
        // surfaced at clone time with a quoting-confused git error
        // far from the source caixa.lisp. The new gate moves the
        // check to caixa-build time and names the offending dep +
        // pin + value verbatim.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0 ".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            nome,
            pin,
            value,
            reason,
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(pin, ":tag");
        assert_eq!(value, "v0.1.0 ");
        assert!(
            reason.contains("whitespace"),
            "reason must surface the whitespace arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_tag_carrying_lock_suffix() {
        // The `.lock` suffix is git's atomic-rename guard for
        // in-flight ref updates — a refname ending in `.lock` is
        // unwritable on disk. Pinned separately from the whitespace
        // arm so a future relaxation that admits one but not the
        // other surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0.lock".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":tag");
        assert_eq!(value, "v0.1.0.lock");
        assert!(
            reason.contains(".lock"),
            "reason must surface the .lock arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_branch_carrying_embedded_space() {
        // The canonical "branch name with spaces" footgun (`feature
        // foo`, `release branch`) — git's refname parser rejects raw
        // whitespace, and the failure surfaces at `git checkout
        // 'feature foo'` time with a quoting-confused error far from
        // the source caixa.lisp. Pinned on the `:branch` axis so the
        // gate-applies-to-both-:tag-and-:branch contract is a build-
        // error to relax.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: None,
            branch: Some("feature/foo bar".into()),
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":branch");
        assert_eq!(value, "feature/foo bar");
        assert!(
            reason.contains("whitespace"),
            "reason must surface the whitespace arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_branch_carrying_qualified_prefix() {
        // The `refs/heads/main` shape — the canonical "I copied the
        // fully-qualified ref out of `git show-ref` instead of the
        // leaf" footgun. The caixa-resolver prepends `refs/heads/`
        // at clone time, so this resolves to a literal ref named
        // `refs/heads/refs/heads/main` on disk; the silent double-
        // prefix is the load-bearing reason to gate at validate.
        // The diagnostic must enumerate the leaf the author probably
        // meant (`"main"`) so the fix is one edit.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: None,
            branch: Some("refs/heads/main".into()),
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":branch");
        assert_eq!(value, "refs/heads/main");
        assert!(
            reason.contains("fully-qualified"),
            "reason must surface the qualified-prefix arm, got {reason:?}"
        );
        assert!(
            reason.contains("\"main\""),
            "reason must quote the leaf the author probably meant, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_tag_carrying_qualified_prefix() {
        // Sibling arm of the qualified-prefix gate on the `:tag`
        // axis (`refs/tags/v0.1.0` — same `git show-ref` output-leak
        // footgun). Pinned separately so a future relaxation that
        // only catches the `:branch` arm surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("refs/tags/v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":tag");
        assert_eq!(value, "refs/tags/v0.1.0");
        assert!(
            reason.contains("fully-qualified"),
            "reason must surface the qualified-prefix arm, got {reason:?}"
        );
        assert!(
            reason.contains("\"v0.1.0\""),
            "reason must quote the leaf the author probably meant, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_branch_named_at() {
        // The bare `@` is git's alias for `HEAD`; a `:branch "@"` is
        // unsourceable. Pinned so a future relaxation that admits
        // any single-character refname surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: None,
            branch: Some("@".into()),
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape { pin, value, .. } = err else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":branch");
        assert_eq!(value, "@");
    }

    #[test]
    fn validate_rejects_git_fonte_with_tag_carrying_double_dot() {
        // Git's `<rev1>..<rev2>` range grammar reserves `..` —
        // a `:tag "../escape"` (path-traversal-shaped slug) silently
        // passes parse and surfaces as a refname-parse error or, on
        // older git, a literal `../escape` checkout that escapes the
        // refs/ directory tree. Pinned separately from the
        // qualified-prefix arm so a future relaxation that catches
        // one but not the other surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("../escape".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape { pin, value, .. } = err else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":tag");
        assert_eq!(value, "../escape");
    }

    #[test]
    fn validate_accepts_git_fonte_with_hierarchical_branch() {
        // The positive-control pin: hierarchical refnames with one or
        // more `/` separators (the `feature/foo` / `user/jdoe/feat`
        // canonical idiom) round-trip through the gate. Pinned
        // separately from the leaf-`"main"` positive control so a
        // future tightening that rejects all multi-component refnames
        // surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: None,
            branch: Some("feature/checkout-rewrite".into()),
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_accepts_git_fonte_with_prerelease_tag() {
        // The positive-control pin: semver pre-release shape
        // (`v0.1.0-alpha.1`) — the in-component dot is allowed
        // (only consecutive `..` and trailing `.` are rejected), the
        // mid-component hyphen is allowed. Pinned separately from
        // the bare-`"v0.1.0"` positive control so a future tightening
        // that rejects pre-release tags surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0-alpha.1".into()),
            rev: None,
            branch: None,
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_rejects_git_fonte_with_rev_carrying_refname_unfriendly_value() {
        // The `:rev` axis is routed through `crate::render::is_git_oid`
        // (a SHA's character set is `[0-9a-f]`, not a refname's), so a
        // value with refname-shape punctuation (here, a `:` mid-string
        // — would be a refname violation under `is_git_ref_name` too)
        // is rejected at the OID-shape gate. The two predicates
        // partition the `:fonte` pin axes structurally: an `:rev` value
        // that's a valid refname (`:rev "main"`, `:rev "v0.1.0"`) is
        // *still* rejected here because every refname character outside
        // `[0-9a-f]` fails the OID gate. Same shape as
        // `fonte_pin_shape_diagnostic_carries_offending_nome_pin_value`
        // on the refname-shaped axes — the diagnostic names the
        // offending dep + pin + value verbatim. The flip-from-accept
        // case the prior `:tag`/`:branch` gate left as a "future axis"
        // (e70d213) — now landed.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("c0ffee:notarefname".into()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            nome,
            pin,
            value,
            reason,
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(pin, ":rev");
        assert_eq!(value, "c0ffee:notarefname");
        assert!(
            !reason.is_empty(),
            "FontePinShape `reason` must carry the predicate's wording verbatim"
        );
    }

    #[test]
    fn validate_accepts_git_fonte_with_rev_full_sha1() {
        // The positive-control pin on the SHA-1 OID width: exactly 40
        // lowercase hex characters — the canonical `git rev-parse HEAD`
        // emission on a SHA-1-hashed repository (the default on every
        // pre-2.42 git and the canonical pleme-io substrate hash).
        // Pinned separately from the SHA-256 positive control so a
        // future tightening that only admits one width surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("0123456789abcdef0123456789abcdef01234567".into()),
            branch: None,
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_accepts_git_fonte_with_rev_full_sha256() {
        // The positive-control pin on the SHA-256 OID width: exactly
        // 64 lowercase hex characters — `git`'s
        // `extensions.objectFormat = sha256` emission (GA since Git
        // 2.42 / Oct 2023). The substrate admits either canonical
        // width so an `:rev` authored against a SHA-256-hashed
        // upstream round-trips through the gate without per-repo
        // configuration. Pinned separately from the SHA-1 positive
        // control so a future tightening that drops one width surfaces
        // here as a structural decision.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()),
            branch: None,
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_rejects_git_fonte_with_rev_abbreviated_prefix() {
        // The canonical `git log --short` / `git rev-parse --short HEAD`
        // paste-from-release-notes footgun: a 7-char prefix (git's
        // default `core.abbrev`) silently passes string emptiness
        // checks and resolves to one commit today, but becomes ambiguous
        // tomorrow as the repo grows. Until this gate landed the empty-
        // pin arm passed (the string isn't empty) and the resolver
        // accepted the prefix through git's separate prefix-lookup pass
        // — defeating the reproducibility contract `:rev` carries vs.
        // `:tag` / `:branch`. The new gate moves the check to caixa-
        // build time and names the offending dep + pin + value verbatim.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("c0ffee0".into()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":rev");
        assert_eq!(value, "c0ffee0");
        assert!(
            reason.contains("abbreviated") || reason.contains("ambiguous"),
            "reason must surface the abbreviation arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_rev_uppercase_hex() {
        // The canonical "I pasted the SHA in uppercase" footgun: `git
        // porcelain` emits OIDs lowercase exclusively, so an uppercase-
        // bearing `:rev` round-trips inconsistently across the
        // resolver's `git fetch <remote> <:rev>` ↔ `git rev-parse HEAD`
        // equality-check pipeline and fails the lacre's content-
        // addressing probe with a confusing case-only diff. Pinned
        // separately from the non-hex arm so a future relaxation that
        // admits one but not the other surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("DEADBEEFCAFEBABE0123456789ABCDEF01234567".into()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":rev");
        assert_eq!(value, "DEADBEEFCAFEBABE0123456789ABCDEF01234567");
        assert!(
            reason.contains("uppercase"),
            "reason must surface the uppercase arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_rev_refname_value() {
        // The cross-axis mis-slot footgun: `:rev "main"` — the author
        // conflated `:rev` (hex commit ID, immutable) and `:branch`
        // (mutable ref pointing at whatever HEAD is today). Until this
        // gate landed the resolver silently dispatched on the value
        // shape ("`main` doesn't look like a SHA, fall back to
        // refname"), defeating the `:rev` reproducibility contract.
        // The new gate rejects every non-hex value on the `:rev` axis,
        // so the `:rev`/`:branch` boundary is structurally enforced —
        // a refname in the `:rev` slot is a build error, not a
        // resolver-time silent reinterpretation.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("main".into()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":rev");
        assert_eq!(value, "main");
        // 4 chars `main` fails the length arm before the character arm,
        // so the diagnostic surfaces the abbreviation wording (same
        // path the `c0ffee0` 7-char fixture lands on); the structural
        // assertion is just that the `:rev "main"` value is rejected.
        assert!(
            !reason.is_empty(),
            "FontePinShape reason must be non-empty for refname-shaped :rev"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_rev_tag_shaped_value() {
        // Sibling cross-axis mis-slot: `:rev "v0.1.0"` — the author
        // conflated `:rev` and `:tag`. Pinned separately from the
        // `:rev "main"` (`:branch` mis-slot) arm so a future relaxation
        // that catches one but not the other surfaces here. The
        // length arm fires first (6 chars ≠ 40 ≠ 64); the structural
        // assertion is just that the cross-axis mis-slot is a build
        // error, regardless of which sub-arm surfaces the diagnostic
        // (`is_git_oid` rejects at the first violation; longer
        // tag-shape values would hit the non-hex arm instead).
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("v0.1.0".into()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":rev");
        assert_eq!(value, "v0.1.0");
        assert!(
            !reason.is_empty(),
            "FontePinShape reason must be non-empty for tag-shaped :rev"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_rev_too_long() {
        // Boundary case on the upper end: 41 hex chars — one past the
        // SHA-1 width, well below the SHA-256 width. Pin so a future
        // relaxation that admits "long enough to be a SHA" without
        // matching either canonical width surfaces here. The diagnostic
        // names the offending length verbatim so the author's grep
        // target is unambiguous (either trim one char or paste the
        // full SHA-256).
        let too_long: String = "0".repeat(41);
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some(too_long.clone()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":rev");
        assert_eq!(value, too_long);
        assert!(
            reason.contains("41"),
            "reason must surface the offending length verbatim, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_rev_carrying_whitespace() {
        // The canonical paste-from-doc footgun on `:rev` — author
        // copies `"deadbeefcafe…0123 "` (trailing space) out of a
        // commit-message paragraph. Until this gate landed the empty-
        // pin arm passed (the string isn't empty), the resolver issued
        // `git fetch <remote> 'deadbeef… '` and the failure surfaced at
        // clone time with a quoting-confused git error far from the
        // source caixa.lisp. The new gate moves the check to caixa-
        // build time. Length is 41 (40 hex + space) so the length arm
        // fires first — pinned separately from the pure-length arm to
        // ensure the diagnostic surfaces *some* parser wording, not
        // silently pass through.
        let with_space = "0123456789abcdef0123456789abcdef01234567 ".to_string();
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some(with_space.clone()),
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FontePinShape {
            pin, value, reason, ..
        } = err
        else {
            panic!("expected FontePinShape, got other variant");
        };
        assert_eq!(pin, ":rev");
        assert_eq!(value, with_space);
        assert!(
            !reason.is_empty(),
            "FontePinShape reason must be non-empty for whitespace-bearing :rev"
        );
    }

    #[test]
    fn fonte_pin_shape_diagnostic_carries_offending_nome_pin_value_for_rev() {
        // Diagnostic-shape pin on the `:rev` axis: every FontePinShape
        // variant on this axis names the offending dep's `:nome` + the
        // `:rev` axis + the offending value verbatim, so the author's
        // grep target is the literal `:rev "<value>"` block in
        // caixa.lisp. Sibling of the `fonte_pin_shape_diagnostic_
        // carries_offending_nome_pin_value` test on the refname-shaped
        // (`:tag` / `:branch`) axes.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:p/x".into(),
            tag: None,
            rev: Some("not-a-sha".into()),
            branch: None,
        });
        let msg = d
            .validate()
            .expect_err(":rev: expected FontePinShape")
            .to_string();
        assert!(
            msg.contains("\"caixa-teia\""),
            ":rev: diagnostic must quote the offending :nome verbatim, got {msg:?}"
        );
        assert!(
            msg.contains(":rev"),
            ":rev: diagnostic must name the offending pin axis, got {msg:?}"
        );
        assert!(
            msg.contains("not-a-sha"),
            ":rev: diagnostic must quote the offending value verbatim, got {msg:?}"
        );
    }

    #[test]
    fn fonte_pin_empty_fires_before_pin_shape() {
        // Order pin: a `Some("")` `:tag` is the more self-locating
        // diagnostic (the author chose an axis but left it blank;
        // grep is unambiguous), so it fires before the shape gate
        // even when both arms would match. Pinned so a future
        // reordering surfaces here. Mirrors the
        // `fonte_repo_empty_fires_before_pin_missing` ordering
        // discipline on the peer per-axis arms.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some(String::new()),
            rev: None,
            branch: None,
        });
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::FontePinEmpty { ref pin, .. } if pin == ":tag"
        ));
    }

    #[test]
    fn fonte_pin_shape_fires_after_repo_empty() {
        // Order pin: `:repo ""` is the more self-locating axis
        // (every git source needs a repo; the per-pin shape gate is
        // secondary), so the repo-empty arm fires before the
        // per-pin shape arm even when both are violated. Pinned so
        // a future reordering surfaces here. Mirrors
        // `fonte_repo_empty_fires_before_pin_missing` on the
        // adjacent axis pair.
        let d = dep_with_fonte(DepSource::Git {
            repo: String::new(),
            tag: Some("v0.1.0 ".into()),
            rev: None,
            branch: None,
        });
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::FonteRepoEmpty { .. }
        ));
    }

    #[test]
    fn fonte_pin_shape_diagnostic_carries_offending_nome_pin_value() {
        // Diagnostic-shape pin across both refname-shaped axes
        // (`:tag` + `:branch`): every `FontePinShape` variant names
        // the offending dep's `:nome` + the offending pin axis + the
        // offending value verbatim, so the author's grep target is
        // unambiguous (the literal `:tag "<value>"` / `:branch
        // "<value>"` lands in caixa.lisp with quotes). Cover both
        // pin axes so a future variant addition forces a parallel
        // diagnostic-shape decision.
        for (pin_label, fonte) in [
            (
                ":tag",
                DepSource::Git {
                    repo: "github:p/x".into(),
                    tag: Some("v0.1.0~1".into()),
                    rev: None,
                    branch: None,
                },
            ),
            (
                ":branch",
                DepSource::Git {
                    repo: "github:p/x".into(),
                    tag: None,
                    rev: None,
                    branch: Some("feature/foo*".into()),
                },
            ),
        ] {
            let d = dep_with_fonte(fonte);
            let msg = d
                .validate()
                .expect_err(&format!("{pin_label}: expected FontePinShape"))
                .to_string();
            assert!(
                msg.contains("\"caixa-teia\""),
                "{pin_label}: diagnostic must quote the offending :nome verbatim, got {msg:?}"
            );
            assert!(
                msg.contains(pin_label),
                "{pin_label}: diagnostic must name the offending pin axis, got {msg:?}"
            );
        }
    }

    #[test]
    fn git_source_json_round_trip() {
        let src = DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        };
        let s = serde_json::to_string(&src).unwrap();
        assert!(s.contains(r#""tipo":"git""#));
        assert!(s.contains(r#""repo":"github:pleme-io/caixa-teia""#));
        assert!(s.contains(r#""tag":"v0.1.0""#));
        assert!(!s.contains("rev"));
        assert!(!s.contains("branch"));
        let round: DepSource = serde_json::from_str(&s).unwrap();
        assert_eq!(round, src);
    }

    // ── per-entry :caracteristicas set-not-multiset gate ────────────
    //
    // Every Vec-keyed-by-name authoring surface on the typed Caixa
    // surface that identifies its entries by a name field now uniformly
    // closes the set-not-multiset discipline at build time (cite
    // `validate_caracteristicas`'s peer-axis enumeration). The
    // `:caracteristicas` axis is per-`Dep`: the feature-toggle slot is
    // set-shaped (a feature is either enabled or not — there is no
    // `feature × 2` semantic), so two entries naming the same feature
    // are a redundant declaration the caixa-resolver's lacre pipeline
    // would silently dedup at resolve time. The empty-feature arm
    // closes the parallel "operationally-meaningless value" axis on
    // the same slot. Same linear-walk + `HashSet` + first-collision
    // shape every peer set gate uses; same empty-first cascade every
    // peer per-entry shape + duplicate gate uses (the empty-feature
    // axis is the more-actionable defect since two `""` entries would
    // both report `caracteristica: ""` under a duplicate-first
    // ordering, with no way to distinguish the offending site).

    fn dep_with_features(features: &[&str]) -> Dep {
        Dep {
            nome: "caixa-teia".into(),
            versao: "^0.1".into(),
            fonte: None,
            opcional: false,
            caracteristicas: features.iter().map(|s| (*s).into()).collect(),
        }
    }

    #[test]
    fn validate_rejects_empty_caracteristica() {
        // Fail-before-pass-after pin: every pre-gate codebase accepted
        // `(:caracteristicas (""))` cleanly (the `Vec<String>` field
        // imposed no per-entry shape contract), the dep validated, and
        // the empty feature would have reached the future caixa-resolver
        // lacre pipeline as a no-op feature enable — silently dropping
        // the author's intent far from the source `caixa.lisp`. The new
        // gate surfaces the structural defect at the typed-validate
        // surface with a self-locating diagnostic naming the offending
        // dep's `:nome`.
        let d = dep_with_features(&[""]);
        assert!(
            matches!(d.validate().unwrap_err(), DepError::CaracteristicaEmpty { ref nome } if nome == "caixa-teia"),
            "expected CaracteristicaEmpty, got {:?}",
            d.validate(),
        );
    }

    #[test]
    fn validate_rejects_duplicate_caracteristica() {
        // Fail-before-pass-after pin on the set-not-multiset arm: the
        // feature-toggle slot is set-shaped, so `(:caracteristicas
        // ("http" "http"))` is a redundant declaration the lacre
        // pipeline dedupes silently at resolve time. The diagnostic
        // names the offending dep + the colliding feature verbatim so
        // the author can grep their caixa.lisp for `:caracteristicas`
        // and fix it in one edit. First-collision determinism is
        // pinned separately below.
        let d = dep_with_features(&["http", "http"]);
        assert!(
            matches!(
                d.validate().unwrap_err(),
                DepError::CaracteristicaDuplicate { ref nome, ref caracteristica }
                    if nome == "caixa-teia" && caracteristica == "http"
            ),
            "expected CaracteristicaDuplicate, got {:?}",
            d.validate(),
        );
    }

    #[test]
    fn validate_accepts_distinct_caracteristicas() {
        // The canonical authoring shape — every feature distinct — must
        // remain a clean pass (positive control sweep). Covers the
        // canonical kebab-case feature names a target caixa typically
        // declares.
        dep_with_features(&["http", "json", "tls"])
            .validate()
            .unwrap();
    }

    #[test]
    fn validate_accepts_single_caracteristica() {
        // Single-element list is the minimum non-empty shape; passes
        // the gate as the identity of the duplicate check (no second
        // entry to collide with).
        dep_with_features(&["http"]).validate().unwrap();
    }

    #[test]
    fn validate_accepts_empty_caracteristicas_list() {
        // The bare-dep authoring shape (`Dep::simple` / `Dep::git`)
        // produces `caracteristicas: Vec::new()`; the empty list is
        // the gate's empty-set identity and passes vacuously. Pin
        // this so a future tightening that requires ≥1 feature
        // surfaces here as a test failure rather than a silent
        // contract narrowing.
        Dep::simple("caixa-teia", "^0.1").validate().unwrap();
        assert!(dep_with_features(&[]).validate().is_ok());
    }

    #[test]
    fn validate_caracteristica_empty_fires_before_duplicate() {
        // Empty-first cascade: an entry with an empty feature *and*
        // duplicate entries surfaces the empty diagnostic first. The
        // empty-feature axis is the more-actionable defect since
        // `caracteristica: ""` is unambiguous; under duplicate-first
        // ordering the diagnostic could report the empty string from
        // either of two empty entries with no way to distinguish.
        // Mirrors the peer empty-before-duplicate ordering
        // discipline every per-entry shape + duplicate gate establishes
        // (`SupervisorSpec::validate`'s `EmptyChildName` arm before
        // `DuplicateChildCaixa`, `validate_membros`'s
        // `MembroCaixaEmpty` arm before `MembroDuplicate`).
        let d = dep_with_features(&["", "http", "http"]);
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::CaracteristicaEmpty { .. }
        ));
    }

    #[test]
    fn validate_caracteristica_duplicate_first_collision_determinism() {
        // Three matching entries: the second occurrence surfaces the
        // diagnostic (the second is the first *collision* — the first
        // entry is the establishing one, not a duplicate). Mirrors
        // every peer first-collision posture
        // (`SupervisorError::DuplicateChildCaixa` reports the second
        // collision, `AplicacaoError::MembroDuplicate` reports the
        // second, `DepError::DuplicateNome` reports the second).
        // Pinning this so a future shortcut that flips to last-
        // collision (or non-deterministic) surfaces here.
        let d = dep_with_features(&["http", "http", "http"]);
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::CaracteristicaDuplicate { ref caracteristica, .. } if caracteristica == "http"
        ));
    }

    #[test]
    fn validate_per_entry_shape_fires_before_caracteristicas() {
        // Per-entry shape precedence: a dep with a malformed `:nome`
        // (uppercase) AND duplicate `:caracteristicas` surfaces the
        // narrower `NomeInvalid` diagnostic first, not the set-gate
        // diagnostic. The `:nome` is the self-locating axis (every
        // diagnostic from the caracteristicas gate quotes the
        // offending dep's `:nome` to anchor the grep target —
        // surfacing the malformed name first keeps that anchor
        // valid). Same precedence shape every peer per-entry-shape
        // arm establishes against its peer set-gate
        // (`validate_deps_per_entry_validate_fires_before_duplicate_in_deps`
        // on the cross-entry `:nome` axis).
        let d = Dep {
            nome: "Caixa-Teia".into(), // uppercase — DNS-1123 violation
            versao: "^0.1".into(),
            fonte: None,
            opcional: false,
            caracteristicas: vec!["http".into(), "http".into()],
        };
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::NomeInvalid { .. }
        ));
    }

    // ── per-entry :caracteristicas value-shape gate ──────────────────
    //
    // Until this gate landed `:caracteristicas` only refused the empty
    // string and cross-entry duplicates: a non-empty distinct but
    // structurally invalid feature name silently passed validate and the
    // failure surfaced at `cargo metadata` time as Cargo's
    // `restricted_names::validate_feature_name` parser rejection, far from
    // the source `caixa.lisp` with no field naming which `:deps` entry's
    // `:caracteristicas` carried the typo. The lifted predicate makes the
    // Cargo-feature-name-grammar intersection-floor a substrate-level
    // invariant at validate time. Same trajectory as the eight peer
    // value-shape predicates each typed surface downstream of a structured
    // grammar already follows.

    #[test]
    fn validate_rejects_caracteristica_with_leading_plus() {
        // Fail-before-pass-after pin on the canonical Cargo
        // `+<feature>` activation-form-in-feature-name-slot footgun.
        // Cargo's `[dependencies.<dep>.features]` list grammar accepts
        // `+optional-feature` as an enablement of a previously-disabled
        // feature; pasting that activation form into `:caracteristicas`
        // (which names the feature itself) silently passed pre-gate and
        // failed at `cargo metadata` parse time.
        let d = dep_with_features(&["+http"]);
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::CaracteristicaInvalid { ref nome, ref caracteristica, .. }
                    if nome == "caixa-teia" && caracteristica == "+http"
            ),
            "expected CaracteristicaInvalid, got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_caracteristica_with_leading_hyphen() {
        // Fail-before-pass-after pin on the leading-hyphen footgun. `-`
        // is a legitimate continuation character (kebab-case feature
        // names like `runtime-tokio` pass) but Cargo rejects it at the
        // start; the structural defect — and its CLI-argument-injection
        // adjacency at any downstream Cargo subprocess invocation — is
        // closed at validate time, not at `cargo metadata` time.
        let d = dep_with_features(&["-json"]);
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::CaracteristicaInvalid { ref caracteristica, .. } if caracteristica == "-json"
            ),
            "expected CaracteristicaInvalid, got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_caracteristica_with_leading_dot() {
        // Fail-before-pass-after pin on the leading-dot footgun. `.` is
        // a legitimate continuation character (version-suffix shapes
        // like `feat.v2` pass) but the leading-dot form is the
        // canonical dotted-version-suffix-as-feature-name confusion.
        let d = dep_with_features(&[".feat"]);
        let err = d.validate().unwrap_err();
        assert!(matches!(
            err,
            DepError::CaracteristicaInvalid { ref caracteristica, .. } if caracteristica == ".feat"
        ));
    }

    #[test]
    fn validate_rejects_caracteristica_with_whitespace() {
        // Fail-before-pass-after pin on the embedded-whitespace footgun:
        // a feature name with a space inside is structurally a multi-
        // token blob (the canonical paste-from-doc footgun, or an
        // accidental `"http server"` where the author meant
        // `"http-server"`).
        let d = dep_with_features(&["http feature"]);
        let err = d.validate().unwrap_err();
        assert!(matches!(
            err,
            DepError::CaracteristicaInvalid { ref caracteristica, .. } if caracteristica == "http feature"
        ));
    }

    #[test]
    fn validate_rejects_caracteristica_with_comma() {
        // Fail-before-pass-after pin on the embedded-comma footgun:
        // the list-separator-belongs-to-the-list-grammar
        // miscomprehension where the author writes
        // `:caracteristicas ("http,json")` intending two features but
        // the `Vec<String>` field consumes the bare token as one entry.
        let d = dep_with_features(&["http,json"]);
        let err = d.validate().unwrap_err();
        assert!(matches!(
            err,
            DepError::CaracteristicaInvalid { ref caracteristica, .. } if caracteristica == "http,json"
        ));
    }

    #[test]
    fn validate_rejects_caracteristica_with_slash() {
        // Fail-before-pass-after pin on the embedded-slash footgun:
        // Cargo's `dep/feat` namespaced-dep syntax applies inside
        // `[dependencies.<dep>.features]` list entries that already
        // name the parent dep (so the syntax says "enable feature
        // `feat` on a transitive dep `dep`"); `:caracteristicas` is
        // per-dep already (a sibling slot on the `Dep` itself), so the
        // segment separator within an entry must be `-`, `_`, `+`,
        // or `.`. The diagnostic remediation points at the canonical
        // Cargo namespaced-dep discipline.
        let d = dep_with_features(&["http/json"]);
        let err = d.validate().unwrap_err();
        assert!(matches!(
            err,
            DepError::CaracteristicaInvalid { ref caracteristica, .. } if caracteristica == "http/json"
        ));
    }

    #[test]
    fn validate_rejects_caracteristica_with_non_ascii() {
        // Fail-before-pass-after pin on the un-percent-encoded non-ASCII
        // byte footgun: NFC-vs-NFD normalization across filesystems
        // silently rewrites the feature-key, breaking the lacre's
        // content-addressing invariant. Pinned at a canonical
        // smart-quote-paste shape (`café`) where the raw `é` byte is the
        // documented APFS round-trip break.
        let d = dep_with_features(&["caf\u{e9}"]);
        let err = d.validate().unwrap_err();
        assert!(matches!(
            err,
            DepError::CaracteristicaInvalid { ref caracteristica, .. } if caracteristica == "caf\u{e9}"
        ));
    }

    #[test]
    fn validate_rejects_caracteristica_with_control_character() {
        // Fail-before-pass-after pin on the embedded-control-character
        // footgun: a CR/LF or any 0x00..0x1F / 0x7F byte landing in a
        // feature name is the canonical paste-from-multiline-doc
        // footgun the predicate's reason wording specifically calls out.
        let d = dep_with_features(&["http\njson"]);
        let err = d.validate().unwrap_err();
        assert!(matches!(err, DepError::CaracteristicaInvalid { .. }));
    }

    #[test]
    fn validate_accepts_canonical_caracteristicas_shapes() {
        // Positive control sweep: every canonical Cargo feature name
        // shape the pleme-io ecosystem uses must still pass. Mirrors
        // the substrate-side `cargo_feature_name_accepts_canonical_forms`
        // sweep — drift between either landing site and the predicate's
        // accepted set is a build error visible at this pair of tests,
        // not a per-renderer "this passed validate but failed at
        // cargo metadata time" surprise on the next acceptance.
        for s in [
            "http",
            "json",
            "derive",
            "serde_json",
            "runtime-tokio",
            "tokio.full",
            "v0.1",
            "http+json",
            "_internal",
            "__private",
            "default",
            "rt-multi-thread",
            "feat.v2",
        ] {
            let d = dep_with_features(&[s]);
            d.validate().unwrap_or_else(|e| {
                panic!("canonical Cargo feature name {s:?} must pass validate: {e:?}")
            });
        }
    }

    #[test]
    fn validate_caracteristica_empty_fires_before_invalid() {
        // Cascade precedence pin: an entry list with both an empty
        // feature AND an invalid-shape feature surfaces the
        // `CaracteristicaEmpty` arm first (the empty value carries no
        // self-locating data — `caracteristica: ""` is the diagnostic
        // with no way to anchor a grep target — so closing the empty
        // axis first preserves the per-entry-shape diagnostic's
        // self-locating discipline). Same empty-first cascade every
        // peer per-entry shape gate establishes
        // (`SupervisorSpec::validate`'s `EmptyChildName` before
        // `ChildCaixaInvalid`, `validate_membros`'s `MembroCaixaEmpty`
        // before `MembroCaixaInvalid`).
        let d = dep_with_features(&["", "+http"]);
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::CaracteristicaEmpty { .. }
        ));
    }

    #[test]
    fn validate_caracteristica_invalid_fires_before_duplicate() {
        // Per-entry-shape precedence pin: an entry list with the same
        // invalid feature shape declared twice surfaces the
        // `CaracteristicaInvalid` diagnostic on the first entry, not
        // the `CaracteristicaDuplicate` on the second collision. The
        // per-entry shape gate fires before the cross-entry set gate
        // — same precedence shape every peer two-arm-plus-set gate
        // establishes (`SupervisorSpec::validate`'s
        // `ChildCaixaInvalid` before `DuplicateChildCaixa`,
        // `validate_membros`'s `MembroCaixaInvalid` before
        // `MembroDuplicate`, `Dep::validate`'s `NomeInvalid` before
        // cross-list `DuplicateNome`).
        let d = dep_with_features(&["+http", "+http"]);
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::CaracteristicaInvalid { ref caracteristica, .. } if caracteristica == "+http"
        ));
    }

    #[test]
    fn validate_rejects_caracteristica_at_65_byte_boundary() {
        // Boundary pin on the 64-byte cap — both the boundary-accepting
        // case and the boundary-exceeding case in one place, so a
        // future cap shift surfaces both arms simultaneously, mirroring
        // the peer `cargo_feature_name_rejects_at_65_byte_boundary`
        // predicate-level pin at the dep-axis landing site.
        let max_ok = "a".repeat(64);
        dep_with_features(&[&max_ok])
            .validate()
            .unwrap_or_else(|e| panic!("64-byte feature name must pass: {e:?}"));
        let too_long = "a".repeat(65);
        let d = dep_with_features(&[&too_long]);
        assert!(matches!(
            d.validate().unwrap_err(),
            DepError::CaracteristicaInvalid { .. }
        ));
    }

    // ── self-dep cross-slot gate ─────────────────────────────────────

    #[test]
    fn validate_no_self_dep_rejects_self_in_deps() {
        // A caixa whose `:deps` lists its own `:nome` is a one-node
        // cycle in the lacre closure's dep-graph traversal — rejected,
        // naming the parent and the offending list tag.
        let deps = vec![
            Dep::simple("caixa-teia", "^0.1"),
            Dep::simple("orquestra", "^0.1"),
        ];
        let err = validate_no_self_dep(&deps, &[], "orquestra").unwrap_err();
        assert!(
            matches!(err, DepError::DepIsSelf { ref nome, list } if nome == "orquestra" && list == ":deps"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_no_self_dep_rejects_self_in_deps_dev() {
        // Same gate on the `:deps-dev` axis — neither dep list is a
        // second-class citizen on the self-edge invariant.
        let deps_dev = vec![Dep::simple("orquestra", "^0.1")];
        let err = validate_no_self_dep(&[], &deps_dev, "orquestra").unwrap_err();
        assert!(
            matches!(err, DepError::DepIsSelf { ref nome, list } if nome == "orquestra" && list == ":deps-dev"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_no_self_dep_deps_fires_before_deps_dev() {
        // Walk order pin: a caixa that self-references on both lists
        // surfaces the `:deps` arm first — the load-bearing axis the
        // lacre closure resolves at every build. Mirrors the canonical
        // [`Caixa::validate_deps`] cascade (`:deps` → `:deps-dev`).
        let deps = vec![Dep::simple("orquestra", "^0.1")];
        let deps_dev = vec![Dep::simple("orquestra", "^0.2")];
        let err = validate_no_self_dep(&deps, &deps_dev, "orquestra").unwrap_err();
        assert!(
            matches!(err, DepError::DepIsSelf { ref nome, list } if nome == "orquestra" && list == ":deps"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_no_self_dep_accepts_distinct_names() {
        // Positive control: every dep names a distinct caixa. The
        // canonical author surface — peer of
        // [`validate_no_self_supervision_accepts_distinct_children`].
        let deps = vec![
            Dep::simple("caixa-teia", "^0.1"),
            Dep::simple("caixa-arch", "^0.1"),
        ];
        let deps_dev = vec![Dep::simple("caixa-test", "^0.1")];
        validate_no_self_dep(&deps, &deps_dev, "orquestra").unwrap();
    }

    #[test]
    fn validate_no_self_dep_empty_lists_pass() {
        // A caixa with no declared deps has nothing to self-reference —
        // the gate is vacuously satisfied. Peer of
        // [`validate_no_self_supervision_empty_children_is_ok`].
        validate_no_self_dep(&[], &[], "orquestra").unwrap();
    }

    #[test]
    fn validate_no_self_dep_diagnostic_carries_offending_list_and_nome() {
        // Diagnostic-shape pin (peer with
        // [`validate_no_self_supervision`]'s diagnostic): the error's
        // Display surfaces both the offending list tag and the
        // parent's `:nome` verbatim, so the author can grep their
        // caixa.lisp for the offending block in one edit. Names
        // `:bibliotecas` / `:exe` / `:servicos` as the corrective
        // surface — every legitimate "I want to use code from this
        // caixa" intent routes through one of those three slots.
        let deps_dev = vec![Dep::simple("orquestra", "^0.1")];
        let rendered = validate_no_self_dep(&[], &deps_dev, "orquestra")
            .unwrap_err()
            .to_string();
        assert!(
            rendered.contains(":deps-dev"),
            "diagnostic must name the offending list tag: {rendered}",
        );
        assert!(
            rendered.contains("orquestra"),
            "diagnostic must quote the parent caixa name: {rendered}",
        );
        assert!(
            rendered.contains(":bibliotecas"),
            "diagnostic must point at the corrective code-surface slot: {rendered}",
        );
    }

    #[test]
    fn validate_no_self_dep_accepts_coincidental_substring_match() {
        // Identity is exact-string equality, not substring — a dep
        // named `"orquestra-helper"` is a distinct caixa even when the
        // parent is `"orquestra"`. Pin the exact-match discipline so a
        // future relaxation that uses `contains` surfaces here, peer
        // with the supervision-tree and Aplicacao-membership gates
        // which all use exact-string equality on the typed identity.
        let deps = vec![Dep::simple("orquestra-helper", "^0.1")];
        validate_no_self_dep(&deps, &[], "orquestra").unwrap();
    }
}
