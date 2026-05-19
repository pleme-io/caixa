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
                // Per-pin value-shape gate for the refname-shaped axes
                // (`:tag` + `:branch`). The `:rev` axis is intentionally
                // skipped: its author-surface shape is a hex commit-ID
                // (`[0-9a-f]+`), not a refname; routing it through
                // [`crate::render::is_git_ref_name`] would admit
                // `:rev "main"`, defeating the reproducibility contract
                // `:rev` carries vs. `:tag` / `:branch`. A dedicated
                // `is_git_oid` predicate on the parallel hex-shape
                // trajectory is a separate future axis.
                //
                // Until this gate landed `:tag` / `:branch` were the last
                // `:fonte`-related axes still untyped past the empty-pin
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
                // disk) silently passed validate and the failure
                // surfaced at lacre-resolve `git fetch` / `git checkout`
                // time with a quoting-confused error far from the source
                // caixa.lisp, with no field naming which `:deps` entry
                // carried the typo. Lifting the gate to caixa-build time
                // matches the value-shape trajectory the peer typed
                // axes already follow (c4213a4 typed WitContract
                // endpoint/subject/slot; eb3456d :entrada :paths;
                // c7d05ec :entrada :host; 4f0390b :contratos :endpoint;
                // 6226bf4 :contratos :wit; 63e18a0 :contratos :subject;
                // 2f4316e :contratos :slot) — the typed slot's valid
                // set matches its downstream consumer's accepted set
                // (here, the git porcelain's refname grammar at
                // `git fetch` / `git checkout` time), structurally.
                // Same diagnostic shape every per-axis value-shape lift
                // already exposes (`*Invalid { axis, reason }`); the
                // `value:` field carries the offending refname verbatim
                // so the author can grep their caixa.lisp for the
                // `:tag "<value>"` / `:branch "<value>"` literal and
                // fix it in one edit.
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
        Ok(())
    }
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
        ":deps entry {nome:?} :fonte (:tipo git …) {pin} {value:?} is not a \
         valid git ref name: {reason} (the git porcelain enforces the same \
         shape at `git fetch` / `git checkout` time on every pin; use a \
         leaf refname like `\"v0.1.0\"` for `:tag` or `\"main\"` / \
         `\"feature/foo\"` for `:branch` — drop any `refs/heads/` or \
         `refs/tags/` prefix the caixa-resolver prepends at clone time)"
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
        // accepts :tag surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("c0ffee0123abc".into()),
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
        // edit. Cover all five variants so a future variant addition
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
    fn validate_accepts_git_fonte_with_rev_carrying_refname_unfriendly_value() {
        // The `:rev` axis is intentionally NOT routed through
        // `is_git_ref_name` (a SHA's character set is `[0-9a-f]`,
        // not a refname's). The gate must not regress
        // `:rev` validation — pin the boundary by feeding a value
        // that would be a refname violation (a `:` mid-string) and
        // verifying the existing single-pin shape passes through.
        // When a future `is_git_oid` gate lands, this test flips to
        // the rejection arm; until then, the omission is a structural
        // decision pinned at this one place.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia".into(),
            tag: None,
            rev: Some("c0ffee:notarefname".into()),
            branch: None,
        });
        d.validate().unwrap();
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
}
