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
            Self::Path { caminho } => Self::validate_caminho(nome, caminho),
        }
    }

    /// Reproducibility + path-API gate on the `:fonte (:tipo path …)`
    /// `:caminho` axis. Walks the leading-byte cascade closed by the
    /// b94fd83 (`/`), a5c248e (`~`), and f4efe9c (`$`) arms; the
    /// orthogonal embedded-control-byte arm (d624c8d) covering
    /// `0x00..=0x1F` plus `0x7F` anywhere in the value; and the
    /// embedded-`\` Windows-path-separator arm closing the
    /// cross-host-OS-separator divergence vector on the same
    /// THEORY.md §V.2 render-determinism axis.
    ///
    /// Extracted from [`Self::validate`]'s `Self::Path` arm because the
    /// per-arm cascade now spans nine diagnostic shapes — every new
    /// `:caminho` arm (a future `&` / `;` / `|` shell-metachar arm,
    /// a future glob-metachar `*` / `?` arm) lands here rather than
    /// re-inflating `Self::validate`. The
    /// function stays a thin per-arm linear walk for one reason: each
    /// arm's diagnostic carries a distinct typed [`DepError`] variant
    /// rather than a parser-shaped `reason` string, so collapsing the
    /// cascade onto a generic [`crate::render`] predicate would regress
    /// the per-arm self-locating diagnostic that `feira lint` consumers
    /// depend on. The wrapped predicate trajectory ([`crate::render::is_dns_1123_label`],
    /// [`crate::render::is_git_repo_url`], etc.) lives on the
    /// reason-string-shaped axes; the `:caminho` axis keeps its
    /// per-arm variant shape.
    #[allow(
        clippy::too_many_lines,
        reason = "the per-arm cascade is structurally flat by design — every \
                  `:caminho` arm carries its own typed [`DepError`] variant + \
                  per-arm Why comment, so collapsing the cascade onto a generic \
                  [`crate::render`] predicate would regress the per-arm self-locating \
                  diagnostic the `feira lint` consumer surface depends on"
    )]
    fn validate_caminho(nome: &str, caminho: &str) -> Result<(), DepError> {
        if caminho.is_empty() {
            return Err(DepError::FonteCaminhoEmpty {
                nome: nome.to_string(),
            });
        }
        // Reproducibility gate on the `:fonte (:tipo path …)`
        // `:caminho` axis. The lacre pipeline embeds the value
        // verbatim in its per-dep content-address
        // (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189) and that string
        // folds into the BLAKE3 closure the lacre keys every
        // downstream consumer (the substrate's reproducibility
        // contract, CAIXA-SDLC §III.2 — the lacre is the
        // build's content-addressed identity, peer of the Nix
        // store path) against. Until this gate landed an
        // absolute `:caminho` (`/home/me/work/caixa-teia` — the
        // canonical "I dragged the folder out of Finder into
        // my editor" footgun; `/Users/alice/dev/caixa-teia` on
        // the macOS path-layout peer; the
        // `${WORKSPACE}/caixa-teia` shell-expanded literal
        // pasted from a CI manifest) silently passed validate
        // and the failure surfaced *as a successful build with
        // a divergent lacre*: the BLAKE3 closure on Alice's
        // workstation differed from the closure on Bob's
        // workstation, two CI runners with different
        // `${HOME}` layouts emitted two distinct
        // content-addresses for the byte-identical caixa, and
        // the substrate's "the lacre is the build's identity"
        // contract silently broke far from the source
        // caixa.lisp — the most insidious failure mode the
        // typed slot can carry (no error surfaces; the
        // divergence is invisible until two machines compare
        // lacres). The same THEORY.md §V.2 render-determinism
        // discipline `is_sandboxed_relative_path` already
        // applies on the M2 typed path-slots
        // (`:behavior :on-*`, `:upgrade-from :state-change
        // :script`, `:bibliotecas`, `:exe`, `:servicos`), here
        // narrowed to the absolute-vs-relative axis only:
        // `:fonte :caminho`'s canonical author-surface form is
        // the `..`-traversing sibling-workspace path
        // (`"../caixa-teia"`, the in-tree dev-dep frame), so a
        // full `is_sandboxed_relative_path` lift would
        // structurally reject every legitimate path-fonte
        // dep. The narrower
        // `std::path::Path::is_absolute` cut admits the
        // sibling-workspace form while still rejecting the
        // host-layout-leaking absolute shape — the
        // reproducibility contract bites at exactly the
        // absolute boundary, and that's the axis the
        // substrate-level invariant is meant to hold. Same
        // diagnostic shape every per-axis value-shape lift on
        // the surrounding [`DepError::Fonte*`] cluster carries
        // (the offending `:nome` + offending `:caminho`
        // quoted verbatim so the author can grep their
        // caixa.lisp for the `:caminho "<value>"` literal and
        // fix it in one edit). The empty arm strictly
        // precedes this arm so the blank-string footgun
        // surfaces the more self-locating
        // `FonteCaminhoEmpty` diagnostic (the empty string
        // is not absolute under `Path::new("").is_absolute()`
        // so the precedence is a no-op at value level — the
        // pin matters only at the diagnostic-shape level if
        // a future codec round-trip ever produces an empty
        // string that probes as absolute).
        if std::path::Path::new(caminho).is_absolute() {
            return Err(DepError::FonteCaminhoAbsolute {
                nome: nome.to_string(),
                caminho: caminho.to_string(),
            });
        }
        // Reproducibility gate's tilde-expansion arm. The b94fd83
        // `FonteCaminhoAbsolute` closes the leading-`/`
        // host-layout-leak; a `:caminho "~/work/caixa-teia"` (the
        // canonical paste-from-shell-prompt / paste-from-`cd ~`-
        // doc footgun) silently passed both the empty arm and
        // the absolute arm because `Path::new("~").is_absolute()`
        // returns `false` — `~` is a shell-expansion convention,
        // not a POSIX path component, so `std::path::Path` treats
        // it as a literal directory-name segment. The lacre
        // pipeline then embedded the value verbatim
        // (`conteudo: format!("path:~/work/caixa-teia")`) and the
        // failure mode forked per consumer:
        //
        //   - The caixa-resolver's `Path` arm folds `:caminho`
        //     through `Path::new(caminho).join(<file>)` without
        //     `~`-expansion, so the build looked for a literal
        //     `./~/work/caixa-teia` subdirectory and failed at
        //     resolve time with a `No such file or directory`
        //     error far from the source caixa.lisp (the lacre
        //     itself, though, was already byte-identical across
        //     machines — every machine emitted the same
        //     `path:~/work/caixa-teia` content-address).
        //   - A future caixa-resolver pass that *does* expand `~`
        //     (the canonical shell-convention idiom every
        //     resolver eventually reaches for once an author
        //     reports the literal-`~`-directory bug) would re-
        //     introduce the host-layout-leak the b94fd83 absolute
        //     gate closes: Alice's `~` expands to `/home/alice`,
        //     Bob's to `/home/bob`, two CI runners with different
        //     `$HOME` layouts resolve to two distinct paths for
        //     the byte-identical caixa, and the substrate's
        //     "the lacre is the build's identity" contract
        //     silently breaks far from the source caixa.lisp.
        //
        // Closing the gate at `DepSource::validate` (here at the
        // canonical caixa-build-time boundary, peer with the
        // absolute arm above) refuses both failure modes
        // structurally: the typed accepted set excludes every
        // `~`-prefixed authoring shape, so the resolver is
        // free to grow `~`-expansion (or any other convention-
        // expansion the substrate adopts) without re-opening
        // the host-layout-leak at the typed boundary. Same
        // diagnostic shape every per-axis value-shape gate on
        // the surrounding [`DepError::Fonte*`] cluster carries
        // (the offending `:nome` + offending `:caminho` quoted
        // verbatim so the author can grep their caixa.lisp for
        // the `:caminho "<value>"` literal and fix it in one
        // edit).
        //
        // The cascade preserves narrower-diagnostic-first
        // ordering: `FonteCaminhoEmpty` → `FonteCaminhoAbsolute`
        // → `FonteCaminhoTildeExpansion`. The empty arm
        // structurally precedes both (the bytes "" / "~" don't
        // overlap), and the absolute arm structurally precedes
        // the tilde arm (an absolute path can't start with `~`
        // since absolute paths start with `/`; the bytes "/" /
        // "~" don't overlap either). Both arms are
        // value-disjoint, so the precedence is a no-op at value
        // level — the pin matters only at the diagnostic-shape
        // level if a future codec round-trip ever produces a
        // value that probes as both absolute and tilde-prefixed.
        if caminho.starts_with('~') {
            return Err(DepError::FonteCaminhoTildeExpansion {
                nome: nome.to_string(),
                caminho: caminho.to_string(),
            });
        }
        // Reproducibility gate's shell-variable-expansion arm.
        // The b94fd83 `FonteCaminhoAbsolute` closes the leading-`/`
        // host-layout-leak; the a5c248e `FonteCaminhoTildeExpansion`
        // closes the leading-`~` shell-home-expansion shape; the
        // leading-`$` is the sibling shell-variable-expansion shape
        // — same host-layout-leaking semantic, different syntactic
        // surface. A `:caminho "$HOME/work/caixa-teia"` (the
        // canonical paste-from-`echo $HOME`-doc footgun) and the
        // `${VAR}`-braced variant (`"${WORKSPACE}/caixa-teia"` —
        // the canonical paste-from-CI-manifest footgun every
        // GitHub Actions / GitLab CI / Drone manifest carries)
        // silently passed every prior arm because
        // `Path::is_absolute` returns false on `$` (the `$` is a
        // shell convention, not a POSIX path component, so
        // `std::path::Path` treats it as a literal directory-name
        // segment) and the tilde arm's `starts_with('~')` doesn't
        // fire.
        //
        // Same per-consumer failure-fork the tilde arm closes:
        //
        //   - The caixa-resolver's `Path` arm folds `:caminho`
        //     through `Path::new(caminho).join(<file>)` without
        //     `$`-expansion, so the build looks for a literal
        //     `./$HOME/work/caixa-teia` subdirectory and fails at
        //     resolve time with a `No such file or directory`
        //     error far from the source caixa.lisp.
        //   - A future caixa-resolver pass that *does* expand
        //     `$VAR` (the shell-convention idiom every resolver
        //     eventually reaches for once an author reports the
        //     literal-`$HOME`-directory bug, especially for CI's
        //     `${WORKSPACE}` idiom) would re-introduce the host-
        //     layout-leak the b94fd83 absolute gate closes:
        //     Alice's `$HOME` expands to `/home/alice`, Bob's to
        //     `/home/bob`, two CI runners with different
        //     `${WORKSPACE}` layouts resolve to two distinct
        //     paths for the byte-identical caixa, and the
        //     substrate's "the lacre is the build's identity"
        //     contract silently breaks far from the source
        //     caixa.lisp.
        //
        // Closing the gate at `DepSource::validate` (here at the
        // canonical caixa-build-time boundary, peer with the
        // absolute + tilde arms above) refuses both failure modes
        // structurally. Same diagnostic shape every per-axis
        // value-shape gate on the surrounding [`DepError::Fonte*`]
        // cluster carries (the offending `:nome` + offending
        // `:caminho` quoted verbatim).
        //
        // The cascade preserves narrower-diagnostic-first ordering:
        // `FonteCaminhoEmpty` → `FonteCaminhoAbsolute` →
        // `FonteCaminhoTildeExpansion` → `FonteCaminhoVarExpansion`.
        // The empty arm structurally precedes all three subsequent
        // arms; the absolute arm structurally precedes both the
        // tilde and the var arms (absolute paths start with `/`,
        // the bytes `/` / `~` / `$` don't overlap at the leading
        // position); the tilde arm structurally precedes the var
        // arm (`~` and `$` don't overlap at the leading position).
        // Every pair is value-disjoint, so the precedence is a
        // no-op at value level — the pin matters only at the
        // diagnostic-shape level if a future codec round-trip ever
        // produces a probe-as-both value.
        //
        // The gate covers every leading-`$` shape: the canonical
        // `"$HOME/work/caixa-teia"` (POSIX shell), the braced
        // `"${HOME}/work/caixa-teia"` (POSIX shell braces), the
        // CI-manifest idiom `"${WORKSPACE}/caixa-teia"` (the
        // GitHub Actions / GitLab CI / Drone paste footgun), the
        // XDG idiom `"$XDG_CONFIG_HOME/caixa"`, and the bare `$`
        // (degenerate "I meant `$HOME` and forgot the rest"). All
        // shapes route through the same `caminho.starts_with('$')`
        // byte check.
        if caminho.starts_with('$') {
            return Err(DepError::FonteCaminhoVarExpansion {
                nome: nome.to_string(),
                caminho: caminho.to_string(),
            });
        }
        // Reproducibility gate's leading-space arm. The b94fd83 / a5c248e /
        // f4efe9c arms closed the leading-byte host-layout-leak shapes
        // (`/` / `~` / `$`); the embedded-control-byte arm below closes
        // every byte in `0x00..=0x1F` plus `0x7F` (which already includes
        // tab `0x09`, LF `0x0A`, CR `0x0D` — every ASCII whitespace
        // *except* the ASCII space byte `0x20`). The bare ASCII space at
        // the leading position is the orthogonal paste-from-aligned-doc
        // shape that silently passed every prior arm: `Path::is_absolute`
        // returns false on `" ../caixa-teia"` (the leading byte is `0x20`
        // not `0x2F`), `0x20` is not `~` / `$` / `\` / a control byte, and
        // the value's last byte is not `/`, so the canonical
        // paste-from-aligned-`caixa.lisp`-doc footgun (every `:fonte`
        // form in a multi-entry `:deps` block sits at the same column —
        // an author selecting `"<sp><sp><sp>../caixa-teia"` and pasting
        // it from the rendered alignment into a fresh entry preserves the
        // leading whitespace verbatim) silently rendered as a path with
        // a leading-space directory component the resolver folds through
        // `Path::join` looking for a literal `./ ../caixa-teia`
        // subdirectory that fails at resolve time with a non-self-
        // locating `No such file or directory` error.
        //
        // The lacre pipeline's reproducibility contract bites
        // strictly at this byte: `path:" ../caixa-teia"` and
        // `path:"../caixa-teia"` yield distinct BLAKE3 closures
        // (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189) for the byte-divergent /
        // semantic-identical caixa, and the substrate's "the lacre is
        // the build's identity" contract (CAIXA-SDLC §III.2) silently
        // breaks across two workstations whose authors differ only in
        // paste-from-aligned-doc whitespace habits — the most insidious
        // failure mode the typed slot can carry (no error surfaces; the
        // divergence is invisible until two machines compare lacres).
        //
        // The arm fires AFTER the absolute / tilde / var leading-byte
        // arms (each names the more self-locating shell-convention
        // diagnostic on values that probe as that arm's leading-byte
        // sentinel followed by a leading space — e.g.
        // `:caminho "/  /foo"` surfaces `FonteCaminhoAbsolute` because
        // the leading byte is `/`, not space) and BEFORE the
        // embedded-control-byte arm (a leading-space value with an
        // embedded control byte surfaces the broader leading-space
        // diagnostic because the cascade walks leading-byte arms first
        // — peer with how `FonteCaminhoAbsolute` precedes
        // `FonteCaminhoControlChar` on `"/etc/passwd\n"`).
        //
        // The peer single-token-shaped axes already reject leading
        // whitespace on the same paste-from-aligned-doc contract:
        // [`crate::render::is_git_repo_url`] rejects leading whitespace
        // on `:fonte :repo`, [`crate::render::is_git_ref_name`] rejects
        // leading whitespace on `:fonte :tag`/`:branch`,
        // [`crate::render::is_chart_description_shape`] rejects leading
        // whitespace on `:descricao`,
        // [`crate::render::is_spdx_expression_shape`] rejects leading
        // whitespace on `:licenca`. Closing the same byte on
        // `:fonte :caminho` makes the substrate-wide "no leading ASCII
        // space anywhere in a typed string slot" invariant structurally
        // consistent across every value-shape-gated typed surface (the
        // `:caminho` axis was the last typed string surface still
        // admitting a leading space byte).
        if caminho.starts_with(' ') {
            return Err(DepError::FonteCaminhoLeadingWhitespace {
                nome: nome.to_string(),
                caminho: caminho.to_string(),
            });
        }
        // Reproducibility gate's embedded-control-byte arm. The
        // b94fd83 + a5c248e + f4efe9c arms closed the three
        // leading-byte host-layout-leak shapes (`/` / `~` / `$`);
        // this arm closes the orthogonal embedded-control-byte
        // axis — any ASCII control byte (`0x00..=0x1F` plus
        // `0x7F` DEL) appearing anywhere in `:caminho`. Same
        // shape every peer single-token-typed-slot value-shape
        // predicate the surrounding [`crate::render`] cluster
        // gates against (the lifted `is_git_repo_url` arm on
        // `:fonte :repo`, the `is_git_ref_name` arm on
        // `:tag`/`:branch`, the `is_chart_description_shape` /
        // `is_chart_maintainer_name_shape` /
        // `is_chart_keyword_shape` arms on the
        // Helm-chart-shaped axes); now consistent on the
        // `:caminho` axis too.
        //
        // Until this gate landed any embedded control byte
        // silently passed validate, the lacre pipeline embedded
        // the value verbatim in its per-dep content-address
        // (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), and the failure
        // forked per byte and per consumer:
        //
        //   - NUL (`0x00`) the canonical "POSIX paths cannot
        //     contain a NUL byte" shape: every `std::fs` syscall
        //     routes the path through `CString::new`, which
        //     fails with `NulError` on the first NUL byte; the
        //     build would surface a `NulError` at resolve time
        //     far from the source caixa.lisp.
        //   - LF (`0x0A`) / CR (`0x0D`) the canonical paste-from-
        //     multiline-doc footgun: a `:caminho
        //     "../caixa-teia\nrm -rf /"` value (paste landed mid-
        //     `:caminho` block from a multi-line code-fence)
        //     silently round-trips through `Path::join` but the
        //     embedded newline class is a sibling of the CRLF-at-
        //     subprocess-argument injection vector
        //     `is_git_repo_url` already closes on `:repo`.
        //   - Tab (`0x09`) the canonical paste-from-aligned-table
        //     footgun: the tab is invisible in most editors, and
        //     the lacre embeds the value verbatim so two
        //     paste-from-distinct-tables yield divergent lacres
        //     across host editors that strip vs preserve tabs.
        //   - DEL (`0x7F`) + every other `0x00..=0x1F` byte: the
        //     paste-from-binary-blob shape every peer single-
        //     token-shaped slot rejects under the same
        //     `b < 0x20 || b == 0x7F` predicate.
        //
        // Mirrors the cascade discipline every prior `:caminho`
        // arm establishes: `FonteCaminhoEmpty` →
        // `FonteCaminhoAbsolute` → `FonteCaminhoTildeExpansion`
        // → `FonteCaminhoVarExpansion` →
        // `FonteCaminhoLeadingWhitespace` → `FonteCaminhoControlChar`.
        // The five leading-byte arms structurally precede the
        // embedded-byte arm because the leading-byte shapes are
        // the more self-locating diagnostic on values that probe
        // as both (e.g. `:caminho "/etc/passwd\n"` surfaces the
        // narrower `FonteCaminhoAbsolute` rather than the broader
        // embedded-control-byte arm); the precedence pin matters
        // at the diagnostic-shape level even though the empty /
        // absolute / tilde / var arms are value-disjoint from a
        // bare control byte (which would itself be a leading
        // byte under the empty / absolute / tilde / var arms'
        // leading-position semantics, but those arms guard the
        // specific shell-convention characters `/` / `~` / `$`
        // — a leading `0x01` byte falls through to this arm).
        for &b in caminho.as_bytes() {
            if b < 0x20 || b == 0x7F {
                return Err(DepError::FonteCaminhoControlChar {
                    nome: nome.to_string(),
                    caminho: caminho.to_string(),
                    byte: b,
                });
            }
        }
        // Reproducibility gate's Windows-path-separator arm. The four
        // leading-byte arms (`/` / `~` / `$`) and the embedded-
        // control-byte arm close the host-layout-leaking + paste-from-
        // multiline-doc shapes; the leading-`\` / embedded-`\` byte is
        // the orthogonal cross-host-OS-separator shape — same render-
        // determinism axis, different semantic mechanism. POSIX
        // [`std::path::Path`] treats `\` (0x5C) as a literal byte
        // inside a single path component (so `..\caixa-teia` is one
        // directory named literally `..\caixa-teia`, sibling of `.`
        // and `..`); Windows [`std::path::Path`] treats `\` as a
        // primary path separator equal to `/` (so `..\caixa-teia` is
        // the parent's sibling directory `caixa-teia`). The lacre
        // pipeline embeds the value verbatim in its per-dep content-
        // address (`conteudo: format!("path:{caminho}")`, caixa-
        // resolver/src/resolve.rs:189), so byte-identical caixa.lisp
        // values resolve to two distinct directories across runner
        // OSes — the same THEORY.md §V.2 render-determinism contract
        // the absolute / tilde / var arms protect, here against the
        // cross-host-OS-separator divergence vector. Even on POSIX-
        // only resolvers (the canonical pleme-io substrate posture),
        // a `..\caixa-teia` (the Windows-Explorer "copy as path" /
        // PowerShell `Get-Location` paste-idiom footgun) silently
        // passes every prior arm because `Path::is_absolute` returns
        // false on `..` and `\` is neither a leading-byte sentinel
        // nor a control byte, then the resolver folds the value
        // through `Path::new(caminho).join(<file>)` looking for a
        // literal `./..\caixa-teia` subdirectory and fails at
        // resolve time with a non-self-locating `No such file or
        // directory` error far from the source caixa.lisp.
        //
        // The peer single-token-shaped axes on the same git-CLI /
        // path-CLI consumer cluster already reject `\` under the same
        // Windows-path-leak banner: [`crate::render::is_git_ref_name`]
        // line 1441 (`"must not contain \\ … the canonical Windows-
        // path-leak footgun; use / for hierarchical refs"`) gates
        // `:fonte :tag` / `:fonte :branch` against the same byte,
        // and [`crate::render::is_gateway_api_http_path`] line 506
        // includes `\` in the eleven-byte RFC-3986-reserved rejection
        // set on `:entrada :paths`. Closing the same byte on `:fonte
        // :caminho` makes the substrate-wide "no Windows path
        // separator anywhere in a typed string slot" invariant
        // structurally consistent across every path-shaped typed
        // surface (the `:caminho` axis was the last typed string
        // surface still admitting `\`).
        //
        // The arm fires AFTER the control-char arm because the
        // control-char diagnostic is the more self-locating axis on
        // values that probe as both (`"..\caixa\0teia"` carries both
        // a `\` and a NUL — NUL is the load-bearing POSIX-syscall-
        // rejected byte, so `FonteCaminhoControlChar` wins). Same
        // narrower-diagnostic-first cascade discipline every prior
        // arm establishes. A pure-`\` value
        // (`"..\caixa-teia"` with no control bytes) falls through
        // every prior arm and lands here.
        for &b in caminho.as_bytes() {
            if b == b'\\' {
                return Err(DepError::FonteCaminhoBackslash {
                    nome: nome.to_string(),
                    caminho: caminho.to_string(),
                });
            }
        }
        // Reproducibility gate's shell-redirection arm. The 3a4e1d7 backslash
        // arm closes the cross-host-OS-separator vector; `<` (`0x3C`) and `>`
        // (`0x3E`) are the orthogonal shell-redirection sentinels — same
        // paste-from-shell-prompt footgun class, different syntactic surface.
        // POSIX `std::path::Path` treats `<` / `>` as literal bytes inside a
        // single path component (so `../caixa-teia>output` is one directory
        // named literally `../caixa-teia>output`, sibling of `.` and `..`),
        // but every interactive shell (bash / zsh / fish / nushell) lexes
        // `<` / `>` as input / output redirection operators — a `:caminho
        // "../caixa-teia>build.log"` (the canonical "I pasted a shell
        // pipeline that wrote build output and forgot to trim the redirect"
        // footgun) or `:caminho "../<input.lisp"` (the symmetric input-
        // redirection paste idiom) silently passes every prior arm because
        // `Path::is_absolute` returns false, `<` / `>` are neither leading-
        // byte sentinels nor control bytes nor `\`, and the value's last byte
        // isn't `/`. The resolver folds the value through
        // `Path::new(caminho).join(<file>)` looking for a literal
        // `./..\caixa-teia>build.log` subdirectory and fails at resolve time
        // with a non-self-locating `No such file or directory` error far
        // from the source caixa.lisp.
        //
        // The lacre pipeline embeds the value verbatim in its per-dep
        // content-address (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), so a `<` / `>` byte lands in
        // the BLAKE3 closure and rides downstream as part of the build's
        // identity. The bytes carry a second class of hazard the prior
        // separator-shaped arms don't: every typed-string slot whose value
        // ever flows verbatim into a shell-spawned subprocess (the caixa-
        // resolver's `git clone` invocation, a future `feira tofu` shell-
        // out, a future operator-side `nix flake check` spawn) is the
        // canonical CRLF-at-subprocess-argument / shell-metachar injection
        // surface that every peer single-token-shaped typed slot already
        // closes. The peer path-shaped axis `[crate::render::is_gateway_api_http_path]`
        // (caixa-core/src/render.rs:506) rejects `<` / `>` as part of its
        // eleven-byte RFC-3986-reserved set on `:entrada :paths`, and the
        // peer git-ref-shaped axis `[crate::render::is_git_ref_name]` rejects
        // `<` / `>` on `:fonte :tag` / `:fonte :branch` under the same
        // shell-metachar-injection banner. The `:caminho` axis was the last
        // typed string surface still admitting these two bytes; this arm
        // closes the gap so the substrate-wide "no shell-redirection
        // metacharacter anywhere in a typed string slot" invariant is now
        // structurally consistent across every path-shaped typed surface.
        //
        // The arm fires AFTER the control-char arm + backslash arm because
        // both prior arms carry more self-locating diagnostics on values
        // that probe as both (`"..\foo<bar"` carries both `\` and `<` — the
        // cross-OS-separator divergence is the load-bearing axis, so the
        // backslash arm wins; `"../foo\n<bar"` carries both LF and `<` —
        // the POSIX-syscall-rejected byte is the load-bearing axis, so the
        // control-char arm wins). The arm fires BEFORE the trailing-`/` arm
        // because the embedded redirection byte is the more semantic-
        // locating axis on probe-as-both values (`"../foo</"` ends in `/`
        // but the load-bearing diagnostic is the embedded `<` shell-
        // redirection — the trailing `/` is the secondary observation, and
        // an author who removes the `<` is likely to also tab-strip the
        // trailing separator).
        for &b in caminho.as_bytes() {
            if b == b'<' || b == b'>' {
                return Err(DepError::FonteCaminhoShellRedirection {
                    nome: nome.to_string(),
                    caminho: caminho.to_string(),
                    byte: b,
                });
            }
        }
        // Reproducibility gate's shell-pipe arm. The e457141 shell-redirection
        // arm closes the `<` / `>` input/output redirection sentinels; `|`
        // (`0x7C`) is the orthogonal shell-pipe sentinel — same paste-from-
        // shell-prompt footgun class, different syntactic surface. POSIX
        // `std::path::Path` treats `|` as a literal path-component byte (so
        // `../caixa-teia|tee` is one directory named literally
        // `../caixa-teia|tee`, sibling of `.` and `..`), but every interactive
        // shell (bash / zsh / fish / nushell) lexes `|` as the pipe operator
        // — a `:caminho "../caixa-teia | grep foo"` (the canonical "I copied a
        // `ls ../caixa-teia | grep` line out of a shell-history block and
        // forgot to trim the pipeline tail" footgun) or `:caminho
        // "../foo||bar"` (the symmetric "I copied a `cmd-a || cmd-b` short-
        // circuit OR line" idiom) silently passes every prior arm because
        // `Path::is_absolute` returns false on `..`, `|` is neither a leading-
        // byte sentinel nor a control byte nor `\` nor `<` / `>`, and the
        // value's last byte isn't `/`. The resolver folds the value through
        // `Path::new(caminho).join(<file>)` looking for a literal
        // `./../caixa-teia | grep foo` subdirectory and fails at resolve time
        // with a non-self-locating `No such file or directory` error far
        // from the source caixa.lisp.
        //
        // The lacre pipeline embeds the value verbatim in its per-dep
        // content-address (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), so a `|` byte lands in the
        // BLAKE3 closure and rides downstream as part of the build's identity
        // into every shell-spawned subprocess (the caixa-resolver's `git
        // clone` invocation, a future `feira tofu` shell-out, a future
        // operator-side `nix flake check` spawn) as the canonical CRLF-at-
        // subprocess-argument / shell-metachar injection surface every peer
        // single-token-shaped typed slot already closes. The peer path-shaped
        // axis [`crate::render::is_gateway_api_http_path`]
        // (caixa-core/src/render.rs:506) rejects `|` as part of its eleven-
        // byte RFC-3986-reserved set on `:entrada :paths`. The `:caminho`
        // axis was the last typed path-string surface still admitting this
        // byte; this arm closes the gap so the substrate-wide "no shell-
        // composition metacharacter anywhere in a typed string slot that
        // flows verbatim into a shell-spawned subprocess" invariant extends
        // from shell-redirection (`<` / `>`) to shell-pipe (`|`) on the
        // `:caminho` axis.
        //
        // The arm fires AFTER the shell-redirection arm because the prior
        // arm's two-byte `byte: u8` payload is the more self-locating axis on
        // values that probe as both (`"../caixa-teia<input|tee"` carries both
        // `<` and `|` — the input-redirection-paste idiom is the load-bearing
        // root-cause edit, so `FonteCaminhoShellRedirection` wins; same
        // cascade discipline every prior `:caminho` arm establishes). The arm
        // fires BEFORE the trailing-`/` arm because the embedded pipe byte is
        // the more semantic-locating axis on probe-as-both values
        // (`"../foo|tee/"` ends in `/` but the load-bearing diagnostic is the
        // embedded `|` shell-pipe — the trailing `/` is the secondary
        // observation, and an author who removes the `|` is likely to also
        // tab-strip the trailing separator).
        for &b in caminho.as_bytes() {
            if b == b'|' {
                return Err(DepError::FonteCaminhoShellPipe {
                    nome: nome.to_string(),
                    caminho: caminho.to_string(),
                });
            }
        }
        // Reproducibility gate's shell-command-separator arm. The 124106f
        // shell-pipe arm closes the `|` byte; `;` (`0x3B`) is the orthogonal
        // shell-command-separator sentinel — same paste-from-shell-prompt
        // footgun class, different syntactic surface. POSIX `std::path::Path`
        // treats `;` as a literal path-component byte (so
        // `../caixa-teia;rm -rf /` is one directory named literally
        // `../caixa-teia;rm -rf /`, sibling of `.` and `..`), but every
        // interactive shell (bash / zsh / fish / nushell) lexes `;` as the
        // sequential-command terminator that fires the next command
        // regardless of the prior command's exit status — a `:caminho
        // "../caixa-teia; rm -rf build"` (the canonical "I pasted a shell
        // one-liner that chained a cleanup tail after the directory name"
        // footgun) or `:caminho "../foo;;bar"` (the symmetric "I copied a
        // POSIX `case` arm's `;;` terminator into the middle of a path"
        // idiom) silently passes every prior arm because `Path::is_absolute`
        // returns false on `..`, `;` is neither a leading-byte sentinel nor a
        // control byte nor `\` nor `<` / `>` nor `|`, and the value's last
        // byte isn't `/`. The resolver folds the value through
        // `Path::new(caminho).join(<file>)` looking for a literal
        // `./../caixa-teia; rm -rf build` subdirectory and fails at resolve
        // time with a non-self-locating `No such file or directory` error far
        // from the source caixa.lisp.
        //
        // The lacre pipeline embeds the value verbatim in its per-dep
        // content-address (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), so a `;` byte lands in the
        // BLAKE3 closure and rides downstream as part of the build's identity
        // into every shell-spawned subprocess (the caixa-resolver's `git
        // clone` invocation, a future `feira tofu` shell-out, a future
        // operator-side `nix flake check` spawn) as the canonical
        // shell-metachar injection surface every peer single-token-shaped
        // typed slot already closes. The peer path-shaped axis
        // [`crate::render::is_gateway_api_http_path`]
        // (caixa-core/src/render.rs:506) rejects `;` as part of its eleven-
        // byte RFC-3986-reserved set on `:entrada :paths`. The `:caminho`
        // axis was the last typed path-string surface still admitting this
        // byte; this arm closes the gap so the substrate-wide "no shell-
        // composition metacharacter anywhere in a typed string slot that
        // flows verbatim into a shell-spawned subprocess" invariant extends
        // from shell-pipe (`|`) to shell-command-separator (`;`) on the
        // `:caminho` axis.
        //
        // The arm fires AFTER the shell-pipe arm because the prior arm's
        // canonical-cmd-a-|-cmd-b shape is the more common shell-history
        // paste idiom on values that probe as both (`"../caixa-teia | tee;
        // rm"` carries both `|` and `;` — the pipeline-tail paste is the
        // load-bearing root-cause edit, so `FonteCaminhoShellPipe` wins; same
        // cascade discipline every prior `:caminho` arm establishes). The arm
        // fires BEFORE the trailing-`/` arm because the embedded
        // command-separator byte is the more semantic-locating axis on
        // probe-as-both values (`"../foo;rm/"` ends in `/` but the
        // load-bearing diagnostic is the embedded `;` shell-command-
        // separator — the trailing `/` is the secondary observation, and an
        // author who removes the `;` is likely to also tab-strip the trailing
        // separator).
        for &b in caminho.as_bytes() {
            if b == b';' {
                return Err(DepError::FonteCaminhoShellSemicolon {
                    nome: nome.to_string(),
                    caminho: caminho.to_string(),
                });
            }
        }
        // Reproducibility gate's shell-background / logical-AND arm. The
        // 05c358e shell-command-separator arm closes the `;` byte; `&`
        // (`0x26`) is the orthogonal shell-background / list-AND sentinel
        // — same paste-from-shell-prompt footgun class, different
        // syntactic surface. POSIX `std::path::Path` treats `&` as a
        // literal path-component byte (so `../caixa-teia & sleep 1` is
        // one directory named literally `../caixa-teia & sleep 1`,
        // sibling of `.` and `..`), but every interactive shell
        // (bash / zsh / fish / nushell) lexes `&` two ways:
        //
        //   - Single `&` as the background-task terminator that detaches
        //     the prior command into the background and returns control
        //     to the prompt immediately (the canonical `cmd &` idiom
        //     every long-running pipeline uses);
        //   - Double `&&` as the logical-AND list operator that fires
        //     the next command only if the prior command succeeded (the
        //     canonical `make && make install` idiom every build script
        //     carries).
        //
        // A `:caminho "../caixa-teia & sleep 1"` (the canonical "I
        // pasted a `cd path & sleep 1` background-launch into the
        // `:caminho` slot" footgun) or `:caminho "../caixa-teia && make"`
        // (the symmetric "I copied a `cd path && make` build chain"
        // idiom) silently passes every prior arm because
        // `Path::is_absolute` returns false on `..`, `&` is neither a
        // leading-byte sentinel nor a control byte nor `\` nor
        // `<` / `>` nor `|` nor `;`, and the value's last byte isn't `/`.
        // The resolver folds the value through
        // `Path::new(caminho).join(<file>)` looking for a literal
        // `./../caixa-teia & sleep 1` subdirectory and fails at resolve
        // time with a non-self-locating `No such file or directory`
        // error far from the source caixa.lisp.
        //
        // The lacre pipeline embeds the value verbatim in its per-dep
        // content-address (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), so an `&` byte lands in
        // the BLAKE3 closure and rides downstream as part of the build's
        // identity into every shell-spawned subprocess (the
        // caixa-resolver's `git clone` invocation, a future `feira tofu`
        // shell-out, a future operator-side `nix flake check` spawn) as
        // the canonical shell-metachar injection surface every peer
        // single-token-shaped typed slot already closes. The peer
        // path-shaped axis [`crate::render::is_gateway_api_http_path`]
        // (caixa-core/src/render.rs:506) rejects `&` as part of its
        // eleven-byte RFC-3986-reserved set on `:entrada :paths`. The
        // `:caminho` axis was the last typed path-string surface still
        // admitting this byte; this arm closes the gap so the
        // substrate-wide "no shell-composition metacharacter anywhere
        // in a typed string slot that flows verbatim into a
        // shell-spawned subprocess" invariant extends from
        // shell-command-separator (`;`) to shell-background /
        // logical-AND (`&`) on the `:caminho` axis.
        //
        // The arm fires AFTER the shell-command-separator arm because
        // the prior arm's `cmd-a; cmd-b` shape is the more common
        // shell-history paste idiom on values that probe as both
        // (`"../caixa-teia; rm & sleep"` carries both `;` and `&` — the
        // command-separator-tail paste is the load-bearing root-cause
        // edit, so `FonteCaminhoShellSemicolon` wins; same cascade
        // discipline every prior `:caminho` arm establishes). The arm
        // fires BEFORE the trailing-`/` arm because the embedded
        // background / list-AND byte is the more semantic-locating axis
        // on probe-as-both values (`"../foo&bar/"` ends in `/` but the
        // load-bearing diagnostic is the embedded `&` shell-background
        // / logical-AND metachar — the trailing `/` is the secondary
        // observation, and an author who removes the `&` is likely to
        // also tab-strip the trailing separator).
        for &b in caminho.as_bytes() {
            if b == b'&' {
                return Err(DepError::FonteCaminhoShellBackground {
                    nome: nome.to_string(),
                    caminho: caminho.to_string(),
                });
            }
        }
        // Reproducibility gate's shell-command-substitution arm. The
        // e12e4f3 shell-background / logical-AND arm closes the `&`
        // byte; the backtick (`0x60`) is the orthogonal POSIX legacy
        // command-substitution sentinel — every POSIX shell (sh /
        // bash / zsh / dash / ksh / fish / nushell) lexes the byte as
        // the canonical legacy wrapper that runs the enclosed command
        // and substitutes its standard-output verbatim into the
        // surrounding word (a `whoami` wrapped in backticks expands
        // to the current user's name; a `cat /etc/passwd` wrapped in
        // backticks expands to the file's contents — the canonical
        // CWE-78 shell-command-injection vector every shell-side
        // hardening guide enumerates first). POSIX
        // `std::path::Path` treats backtick as a literal path-
        // component byte (so `../caixa-teia/<backtick>whoami<backtick>`
        // is one directory named literally that, sibling of `.` and
        // `..`).
        //
        // A `:caminho "../caixa-teia/<backtick>whoami<backtick>"` (the
        // canonical "I pasted a shell one-liner carrying a backticked
        // `whoami` command-substitution expansion into the `:caminho`
        // slot" footgun) or `:caminho "<backtick>pwd<backtick>/caixa-
        // teia"` (the symmetric "I copied a `<backtick>pwd<backtick>/
        // path` working-directory expansion") silently passes every
        // prior arm because `Path::is_absolute` returns false on
        // `..`, the backtick byte is neither a leading-byte sentinel
        // (the f4efe9c `FonteCaminhoVarExpansion` arm catches the
        // modern `$()` form at leading position only; backtick is
        // the orthogonal legacy form) nor a control byte nor `\` nor
        // `<` / `>` nor `|` nor `;` nor `&`, and the value's last
        // byte isn't `/`. The resolver folds the value through
        // `Path::new(caminho).join(<file>)` looking for a literal
        // subdirectory whose name embeds the backticked token and
        // fails at resolve time with a non-self-locating `No such
        // file or directory` error far from the source caixa.lisp.
        //
        // The lacre pipeline embeds the value verbatim in its per-
        // dep content-address (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), so a backtick byte
        // lands in the BLAKE3 closure and rides downstream as part
        // of the build's identity into every shell-spawned
        // subprocess (the caixa-resolver's `git clone` invocation, a
        // future `feira tofu` shell-out, a future operator-side
        // `nix flake check` spawn) as the canonical shell-metachar
        // injection surface every peer single-token-shaped typed
        // slot already closes. The peer path-shaped axis
        // [`crate::render::is_gateway_api_http_path`]
        // (caixa-core/src/render.rs:506) rejects backtick as part of
        // its eleven-byte RFC-3986-reserved set on `:entrada
        // :paths`. The `:caminho` axis was the last typed path-
        // string surface still admitting this byte; this arm closes
        // the gap so the substrate-wide "no shell-composition
        // metacharacter anywhere in a typed string slot that flows
        // verbatim into a shell-spawned subprocess" invariant
        // extends from shell-background / logical-AND (`&`) to
        // shell-command-substitution (backtick) on the `:caminho`
        // axis.
        //
        // The arm fires AFTER the shell-background arm because the
        // prior arm's `cmd & sleep` shape is the more common shell-
        // history paste idiom on values that probe as both (a
        // `"../caixa-teia & <backtick>whoami<backtick>"` carries
        // both `&` and a backtick — the background-launch tail is
        // the load-bearing root-cause edit, so
        // `FonteCaminhoShellBackground` wins; same cascade
        // discipline every prior `:caminho` arm establishes). The
        // arm fires BEFORE the trailing-`/` arm because the
        // embedded command-substitution byte is the more semantic-
        // locating axis on probe-as-both values (a
        // `"../<backtick>whoami<backtick>/"` ends in `/` but the
        // load-bearing diagnostic is the embedded backtick shell-
        // command-substitution metachar — the trailing `/` is the
        // secondary observation, and an author who removes the
        // backtick is likely to also tab-strip the trailing
        // separator).
        for &b in caminho.as_bytes() {
            if b == b'`' {
                return Err(DepError::FonteCaminhoShellCommandSubstitution {
                    nome: nome.to_string(),
                    caminho: caminho.to_string(),
                });
            }
        }
        // Reproducibility gate's shell-glob arm. The c4d62b3 shell-command-
        // substitution arm closes the backtick byte; `*` (`0x2A`) and `?`
        // (`0x3F`) are the orthogonal POSIX glob-expansion sentinels — same
        // paste-from-shell-prompt footgun class, different syntactic surface.
        // Every POSIX shell (sh / bash / zsh / dash / ksh / fish / nushell)
        // lexes `*` and `?` as pathname-expansion wildcards: `*` matches any
        // sequence of characters in a path component (including the empty
        // sequence), `?` matches exactly one character. POSIX
        // `std::path::Path` treats both bytes as literal path-component bytes
        // (so `../caixa-teia/*.lisp` is one directory named literally
        // `../caixa-teia/*.lisp`, sibling of `.` and `..`).
        //
        // A `:caminho "../caixa-teia/*"` (the canonical "I pasted a
        // `ls ../caixa-teia/*` shell-listing one-liner into the `:caminho`
        // slot" footgun) or `:caminho "../foo?"` (the symmetric "I copied a
        // `rm foo?` single-char-wildcard removal idiom") silently passes
        // every prior arm because `Path::is_absolute` returns false on `..`,
        // `*` / `?` are neither leading-byte sentinels nor control bytes nor
        // `\` nor `<` / `>` nor `|` nor `;` nor `&` nor backtick, and the
        // value's last byte isn't `/`. The resolver folds the value through
        // `Path::new(caminho).join(<file>)` looking for a literal
        // `./../caixa-teia/*` subdirectory and fails at resolve time with a
        // non-self-locating `No such file or directory` error far from the
        // source caixa.lisp.
        //
        // The lacre pipeline embeds the value verbatim in its per-dep
        // content-address (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), so a `*` or `?` byte lands in
        // the BLAKE3 closure and rides downstream as part of the build's
        // identity into every shell-spawned subprocess (the caixa-resolver's
        // `git clone` invocation, a future `feira tofu` shell-out, a future
        // operator-side `nix flake check` spawn) as the canonical
        // shell-metachar / pathname-expansion surface every peer
        // single-token-shaped typed slot already closes. The peer path-shaped
        // axis [`crate::render::is_gateway_api_http_path`]
        // (caixa-core/src/render.rs:506) rejects `*` and `?` as part of its
        // eleven-byte RFC-3986-reserved set on `:entrada :paths`. The
        // `:caminho` axis was the last typed path-string surface still
        // admitting these two bytes; this arm closes the gap so the
        // substrate-wide "no shell-composition / glob-expansion
        // metacharacter anywhere in a typed string slot that flows verbatim
        // into a shell-spawned subprocess" invariant extends from
        // shell-command-substitution (backtick) to glob-expansion
        // (`*` / `?`) on the `:caminho` axis.
        //
        // The arm fires AFTER the backtick arm because the prior arm's
        // CWE-78 shell-command-injection vector is the load-bearing
        // diagnostic on values that probe as both (a `"../`whoami`/*"`
        // carries both backtick and `*` — the command-substitution paste
        // is the load-bearing root-cause edit, so
        // `FonteCaminhoShellCommandSubstitution` wins; same cascade
        // discipline every prior `:caminho` arm establishes). The arm
        // fires BEFORE the trailing-`/` arm because the embedded glob
        // byte is the more semantic-locating axis on probe-as-both values
        // (`"../foo*/"` ends in `/` but the load-bearing diagnostic is the
        // embedded `*` glob metachar — the trailing `/` is the secondary
        // observation, and an author who removes the `*` is likely to
        // also tab-strip the trailing separator).
        for &b in caminho.as_bytes() {
            if b == b'*' || b == b'?' {
                return Err(DepError::FonteCaminhoShellGlob {
                    nome: nome.to_string(),
                    caminho: caminho.to_string(),
                    byte: b,
                });
            }
        }
        // Reproducibility gate's trailing-`/` arm. The b94fd83 absolute arm
        // closes the leading-`/` host-layout-leak; the embedded-control-byte
        // arm closes any byte-in-the-`0x00..=0x1F` / `0x7F` range; the
        // backslash arm closes the cross-host-OS-separator vector. The
        // trailing-`/` is the orthogonal shell-tab-completion-on-a-directory
        // footgun — `Path::join("../caixa-teia")` and
        // `Path::join("../caixa-teia/")` resolve to the same directory
        // (POSIX path-component-walk treats trailing `/` as a no-op for
        // directory targets, which `:caminho` always names — the sibling-
        // workspace dep root is structurally a directory). The lacre
        // pipeline embeds the value verbatim in its per-dep content-address
        // (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), so byte-identical caixa
        // semantic-meaning yields two distinct BLAKE3 closures depending on
        // whether the author shell-tab-completed the path (every interactive
        // shell appends `/` on tab-completing a directory, idiomatic in
        // bash / zsh / fish / nushell), pasted from `pwd` (which on most
        // shells emits without trailing `/`, but `realpath -e -m` on a
        // directory with trailing `/` preserves it), or copied a Cargo
        // `path = "../caixa-teia/"` entry from cross-substrate documentation
        // (Cargo accepts both shapes and folds them the same way). Two
        // workstations whose authors differ only in tab-completion habits
        // emit byte-divergent lacres for the byte-identical-semantic caixa,
        // and the substrate's "the lacre is the build's identity" contract
        // (CAIXA-SDLC §III.2) silently breaks far from the source caixa.lisp.
        //
        // Same THEORY.md §V.2 render-determinism axis every prior `:caminho`
        // arm protects, here against the trailing-separator divergence
        // vector: every typed slot's accepted set excludes byte-divergent
        // values that round-trip to the same downstream semantic. The peer
        // path-shaped axes already reject trailing separators on the same
        // contract: [`crate::render::is_gateway_api_http_path`] gates
        // `:entrada :paths` against any non-canonical normalization, and
        // [`crate::render::is_sandboxed_relative_path`] gates the M2 typed
        // path-slots (`:behavior :on-*`, `:upgrade-from :state-change
        // :script`, `:bibliotecas`, `:exe`, `:servicos`) against shapes
        // whose canonical form would re-introduce determinism divergence.
        //
        // The arm fires last in the cascade because every prior arm carries
        // a more self-locating diagnostic on values that probe as both
        // (e.g. `:caminho "../foo/\0/"` ends in `/` but the load-bearing
        // diagnostic is the NUL byte's POSIX-syscall-rejection — the
        // control-char arm wins; `:caminho "/etc/passwd/"` ends in `/` but
        // the load-bearing diagnostic is the absolute host-layout-leak —
        // the absolute arm wins; `:caminho "..\caixa-teia/"` ends in `/`
        // but the load-bearing diagnostic is the Windows-separator cross-
        // OS divergence — the backslash arm wins). The arm covers every
        // shape where the last byte is `/` regardless of length, including
        // the degenerate single-`/` (which the absolute arm catches first)
        // and the consecutive-`//` (where every prior arm passes on the
        // bytes other than the trailing `/`).
        if caminho.as_bytes().last() == Some(&b'/') {
            return Err(DepError::FonteCaminhoTrailingSlash {
                nome: nome.to_string(),
                caminho: caminho.to_string(),
            });
        }
        Ok(())
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
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} is \
         absolute (the lacre pipeline embeds the value verbatim in its \
         per-dep content-address `path:{caminho}` at \
         caixa-resolver/src/resolve.rs:189, so an absolute path makes the \
         BLAKE3 closure differ across machines — defeating the \
         reproducibility contract that's load-bearing for CSE; express \
         the path relative to the caixa.lisp location, e.g. \
         \"../caixa-teia\" for a sibling workspace dep)"
    )]
    FonteCaminhoAbsolute { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} starts \
         with `~` (the leading-tilde is a shell-expansion convention, not a \
         POSIX path component — `Path::is_absolute` returns false on it, so \
         the b94fd83 absolute-path gate doesn't catch it, but the lacre \
         pipeline embeds the value verbatim in its per-dep content-address \
         `path:{caminho}` at caixa-resolver/src/resolve.rs:189 and the \
         caixa-resolver folds it through `Path::join` without `~`-expansion, \
         so the build looks for a literal `./{caminho}` subdirectory and \
         fails at resolve time far from the source caixa.lisp; even worse, a \
         future caixa-resolver pass that *does* expand `~` would silently \
         re-open the host-layout-leak the b94fd83 absolute gate closes — \
         Alice's `~` resolves to `/home/alice`, Bob's to `/home/bob`, two CI \
         runners with different `$HOME` layouts resolve to two distinct paths \
         for the byte-identical caixa, defeating the THEORY.md §V.2 render-\
         determinism contract; express the path relative to the caixa.lisp \
         location, e.g. \"../caixa-teia\" for a sibling workspace dep, or \
         spell out the full relative path explicitly if a workstation-rooted \
         dep is genuinely intended)"
    )]
    FonteCaminhoTildeExpansion { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} starts \
         with `$` (the leading-`$` is a shell-variable-expansion convention, \
         not a POSIX path component — `Path::is_absolute` returns false on it \
         and the a5c248e tilde gate doesn't catch it, but the lacre pipeline \
         embeds the value verbatim in its per-dep content-address \
         `path:{caminho}` at caixa-resolver/src/resolve.rs:189 and the \
         caixa-resolver folds it through `Path::join` without `$`-expansion, \
         so the build looks for a literal `./{caminho}` subdirectory and \
         fails at resolve time far from the source caixa.lisp; even worse, a \
         future caixa-resolver pass that *does* expand `$VAR` (the canonical \
         shell-convention idiom that CI's `${{WORKSPACE}}` paste-idiom \
         invites) would silently re-open the host-layout-leak the b94fd83 \
         absolute gate closes — Alice's `$HOME` resolves to `/home/alice`, \
         Bob's to `/home/bob`, two CI runners with different `${{WORKSPACE}}` \
         layouts resolve to two distinct paths for the byte-identical caixa, \
         defeating the THEORY.md §V.2 render-determinism contract; express \
         the path relative to the caixa.lisp location, e.g. \"../caixa-teia\" \
         for a sibling workspace dep, or spell out the full relative path \
         explicitly if a workstation-rooted dep is genuinely intended)"
    )]
    FonteCaminhoVarExpansion { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} starts \
         with a space (the leading ASCII space `0x20` is the orthogonal \
         paste-from-aligned-doc footgun that silently passes \
         `Path::is_absolute` and every prior leading-byte arm — \
         `\" ../caixa-teia\"` resolves via `Path::join` to a literal \
         `./ ../caixa-teia` subdirectory the resolver fails to find at \
         resolve time with a non-self-locating `No such file or directory` \
         error far from the source caixa.lisp; the lacre pipeline embeds \
         the value verbatim in its per-dep content-address `path:{caminho}` \
         at caixa-resolver/src/resolve.rs:189, so byte-divergent / \
         semantic-identical caixa values (` ../caixa-teia` vs \
         `../caixa-teia`) yield two distinct BLAKE3 closures across two \
         workstations whose authors differ only in paste-from-aligned- \
         caixa.lisp-doc whitespace habits — the most insidious failure \
         mode the typed slot can carry (no error surfaces; the divergence \
         is invisible until two machines compare lacres), defeating the \
         THEORY.md §V.2 render-determinism contract. The canonical \
         paste-from-aligned-`:deps`-block footgun (every `:fonte` form in \
         a multi-entry `:deps` block sits at the same column — an author \
         selecting `\"<sp><sp><sp>../caixa-teia\"` and pasting it from \
         the rendered alignment into a fresh entry preserves the leading \
         whitespace verbatim); peer `:fonte :repo` axis already rejects \
         leading whitespace via `is_git_repo_url`, `:fonte :tag` / \
         `:fonte :branch` via `is_git_ref_name`, `:descricao` via \
         `is_chart_description_shape`, `:licenca` via \
         `is_spdx_expression_shape`. Drop the leading space; express the \
         path as a bare relative single-token like \"../caixa-teia\")"
    )]
    FonteCaminhoLeadingWhitespace { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} contains \
         ASCII control byte 0x{byte:02x} (POSIX paths reject NUL `0x00` outright — \
         every `std::fs` syscall routes the path through `CString::new` which \
         fails with `NulError` at resolve time; the lacre pipeline embeds the \
         value verbatim in its per-dep content-address `path:{caminho}` at \
         caixa-resolver/src/resolve.rs:189 so a control byte anywhere in the \
         value lands in the BLAKE3 closure and breaks the THEORY.md §V.2 render-\
         determinism contract — the canonical paste-from-multiline-doc \
         (`\\n`/`\\r`), paste-from-aligned-table (`\\t`), or paste-from-binary-\
         blob (`0x00`-DEL) footgun every peer single-token-shaped axis \
         (`:fonte :repo`, `:fonte :tag`/`:branch`, the Helm chart-string axes) \
         already gates against. Express the path as a relative single-line ASCII \
         string, e.g. \"../caixa-teia\" for a sibling workspace dep)"
    )]
    FonteCaminhoControlChar {
        nome: String,
        caminho: String,
        byte: u8,
    },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} contains `\\` \
         (POSIX `std::path::Path` treats `\\` as a literal byte inside a single path \
         component, so `..\\caixa-teia` is one directory named literally `..\\caixa-teia` — \
         not the parent's sibling — and the caixa-resolver folds the value through \
         `Path::join` looking for a literal `./{caminho}` subdirectory that fails at \
         resolve time with a non-self-locating `No such file or directory` error far \
         from the source caixa.lisp; Windows `std::path::Path` treats `\\` as a \
         primary path separator equal to `/`, so byte-identical caixa.lisp values \
         resolve to two distinct directories across runner OSes — the lacre pipeline \
         embeds the value verbatim in its per-dep content-address `path:{caminho}` at \
         caixa-resolver/src/resolve.rs:189, defeating the THEORY.md §V.2 render-\
         determinism contract via the cross-host-OS-separator divergence vector. The \
         canonical Windows-Explorer `Copy as path` / PowerShell `Get-Location` \
         paste-idiom footgun; peer `:fonte :tag` / `:fonte :branch` axis already \
         rejects `\\` via `is_git_ref_name` for the same Windows-path-leak reason, \
         and `:entrada :paths` rejects `\\` via `is_gateway_api_http_path`'s eleven-\
         byte RFC-3986-reserved set. Express the path with `/` as the separator, e.g. \
         \"../caixa-teia\" for a sibling workspace dep)"
    )]
    FonteCaminhoBackslash { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} contains shell-\
         redirection metacharacter 0x{byte:02x} `{ch}` (every interactive shell — bash / \
         zsh / fish / nushell — lexes `<` and `>` as input / output redirection \
         operators, so `:caminho \"../caixa-teia>build.log\"` is the canonical \
         paste-from-shell-pipeline footgun where an author copies a `command > log` \
         tail without trimming the redirect; POSIX `std::path::Path` treats both bytes \
         as literal path-component bytes, so the resolver folds the value through \
         `Path::new(caminho).join(<file>)` looking for a literal `./{caminho}` \
         subdirectory and fails at resolve time with a non-self-locating `No such \
         file or directory` error far from the source caixa.lisp. The lacre pipeline \
         embeds the value verbatim in its per-dep content-address `path:{caminho}` at \
         caixa-resolver/src/resolve.rs:189, so the byte lands in the BLAKE3 closure \
         and rides into every shell-spawned subprocess (the resolver's `git clone`, a \
         future `feira tofu` shell-out, a future operator-side `nix` spawn) as the \
         canonical CRLF-at-subprocess-argument / shell-metachar injection surface every \
         peer single-token-shaped typed slot already closes. The peer `:fonte :tag` / \
         `:fonte :branch` axis already rejects `<` / `>` via `is_git_ref_name`, and \
         `:entrada :paths` rejects them via `is_gateway_api_http_path`'s eleven-byte \
         RFC-3986-reserved set. Express the path as a bare relative single-token like \
         \"../caixa-teia\" — the sibling-workspace directory name carries no shell-\
         redirection semantic.",
        ch = *byte as char
    )]
    FonteCaminhoShellRedirection {
        nome: String,
        caminho: String,
        byte: u8,
    },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} contains shell-pipe \
         metacharacter `|` (every interactive shell — bash / zsh / fish / nushell — lexes \
         `|` as the pipe operator that wires one command's stdout to the next command's \
         stdin, so `:caminho \"../caixa-teia | grep foo\"` is the canonical paste-from-\
         shell-history footgun where an author copies a `ls ../caixa-teia | grep` line \
         without trimming the pipeline tail, and `:caminho \"../foo||bar\"` is the \
         symmetric `cmd-a || cmd-b` short-circuit-OR paste shape; POSIX `std::path::Path` \
         treats `|` as a literal path-component byte, so the resolver folds the value \
         through `Path::new(caminho).join(<file>)` looking for a literal `./{caminho}` \
         subdirectory and fails at resolve time with a non-self-locating `No such file or \
         directory` error far from the source caixa.lisp. The lacre pipeline embeds the \
         value verbatim in its per-dep content-address `path:{caminho}` at caixa-resolver/\
         src/resolve.rs:189, so the byte lands in the BLAKE3 closure and rides into every \
         shell-spawned subprocess (the resolver's `git clone`, a future `feira tofu` \
         shell-out, a future operator-side `nix` spawn) as the canonical CRLF-at-\
         subprocess-argument / shell-metachar injection surface every peer single-token-\
         shaped typed slot already closes. The peer `:entrada :paths` axis rejects `|` \
         via `is_gateway_api_http_path`'s eleven-byte RFC-3986-reserved set. Express the \
         path as a bare relative single-token like \"../caixa-teia\" — the sibling-\
         workspace directory name carries no shell-pipe semantic."
    )]
    FonteCaminhoShellPipe { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} contains shell-\
         command-separator metacharacter `;` (every interactive shell — bash / zsh / fish \
         / nushell — lexes `;` as the sequential-command terminator that fires the next \
         command regardless of the prior command's exit status, so `:caminho \
         \"../caixa-teia; rm -rf build\"` is the canonical paste-from-shell-one-liner \
         footgun where an author copies a `cd path; do-thing` chain without trimming \
         the cleanup tail, and `:caminho \"../foo;;bar\"` is the symmetric POSIX `case` \
         arm `;;` terminator paste shape; POSIX `std::path::Path` treats `;` as a \
         literal path-component byte, so the resolver folds the value through \
         `Path::new(caminho).join(<file>)` looking for a literal `./{caminho}` \
         subdirectory and fails at resolve time with a non-self-locating `No such file \
         or directory` error far from the source caixa.lisp. The lacre pipeline embeds \
         the value verbatim in its per-dep content-address `path:{caminho}` at \
         caixa-resolver/src/resolve.rs:189, so the byte lands in the BLAKE3 closure and \
         rides into every shell-spawned subprocess (the resolver's `git clone`, a \
         future `feira tofu` shell-out, a future operator-side `nix` spawn) as the \
         canonical shell-metachar injection surface every peer single-token-shaped \
         typed slot already closes. The peer `:entrada :paths` axis rejects `;` via \
         `is_gateway_api_http_path`'s eleven-byte RFC-3986-reserved set. Express the \
         path as a bare relative single-token like \"../caixa-teia\" — the sibling-\
         workspace directory name carries no shell-command-separator semantic."
    )]
    FonteCaminhoShellSemicolon { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} contains shell-\
         background / list-AND metacharacter `&` (every interactive shell — bash / zsh \
         / fish / nushell — lexes `&` two ways: single `&` as the background-task \
         terminator detaching the prior command and returning control immediately to \
         the prompt, double `&&` as the logical-AND list operator firing the next \
         command only if the prior succeeded; POSIX `std::path::Path` treats it as a \
         literal byte. The canonical paste-from-shell-prompt footgun is a `cd path & \
         sleep 1` background-launch one-liner or a `cd path && make install` build-\
         chain idiom selected whole into the `:caminho` slot — the prior `;` arm at \
         05c358e closed the sequential-command-separator vector, this arm closes the \
         orthogonal background-task / logical-AND vector on the same paste-from-shell-\
         prompt class. The lacre pipeline embeds the value verbatim in its per-dep \
         content-address `path:{caminho}` at caixa-resolver/src/resolve.rs:189, so the \
         byte lands in the BLAKE3 closure and rides into every shell-spawned \
         subprocess (the resolver's `git clone`, a future `feira tofu` shell-out, a \
         future operator-side `nix` spawn) as the canonical shell-metachar injection \
         surface every peer single-token-shaped typed slot already closes. The peer \
         `:entrada :paths` axis rejects `&` via `is_gateway_api_http_path`'s eleven-\
         byte RFC-3986-reserved set. Express the path as a bare relative single-token \
         like \"../caixa-teia\" — the sibling-workspace directory name carries no \
         shell-background / logical-AND semantic."
    )]
    FonteCaminhoShellBackground { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} contains shell-\
         command-substitution metacharacter `` ` `` (every POSIX shell — sh / bash / zsh / \
         dash / ksh / fish / nushell — lexes the byte as the legacy command-substitution \
         wrapper that runs the enclosed command and substitutes its standard-output \
         verbatim into the surrounding word, so a backticked `whoami` expands to the \
         current user's name and a backticked `cat /etc/passwd` expands to the file's \
         contents — the canonical CWE-78 shell-command-injection vector; POSIX \
         `std::path::Path` treats the byte as a literal path-component byte. The canonical \
         paste-from-shell-prompt footgun is a `cd ../path/<backtick>whoami<backtick>` legacy substitution \
         one-liner or a `cd <backtick>pwd<backtick>/path` working-directory expansion idiom selected whole \
         into the `:caminho` slot — the prior `&` arm at e12e4f3 closed the shell-\
         background / logical-AND vector, this arm closes the orthogonal command-\
         substitution vector on the same paste-from-shell-prompt class (the modern `$()` \
         form is gated at leading position by the f4efe9c `FonteCaminhoVarExpansion` arm; \
         the legacy backtick form is the orthogonal axis). The lacre pipeline embeds the \
         value verbatim in its per-dep content-address `path:{caminho}` at \
         caixa-resolver/src/resolve.rs:189, so the byte lands in the BLAKE3 closure and \
         rides into every shell-spawned subprocess (the resolver's `git clone`, a future \
         `feira tofu` shell-out, a future operator-side `nix` spawn) as the canonical \
         shell-metachar injection surface every peer single-token-shaped typed slot \
         already closes. The peer `:entrada :paths` axis rejects the byte via \
         `is_gateway_api_http_path`'s eleven-byte RFC-3986-reserved set. Express the path \
         as a bare relative single-token like \"../caixa-teia\" — the sibling-workspace \
         directory name carries no shell-command-substitution semantic."
    )]
    FonteCaminhoShellCommandSubstitution { nome: String, caminho: String },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} contains shell-\
         glob / pathname-expansion metacharacter 0x{byte:02x} `{ch}` (every POSIX shell — \
         sh / bash / zsh / dash / ksh / fish / nushell — lexes `*` and `?` as pathname-\
         expansion wildcards: `*` matches any sequence of characters in a path component \
         and `?` matches exactly one character, so a `:caminho \"../caixa-teia/*\"` is the \
         canonical paste-from-shell-listing footgun where an author copies a \
         `ls ../caixa-teia/*` listing without trimming the wildcard, and `:caminho \
         \"../foo?\"` is the symmetric single-char-wildcard paste shape; POSIX \
         `std::path::Path` treats both bytes as literal path-component bytes, so the \
         resolver folds the value through `Path::new(caminho).join(<file>)` looking for \
         a literal `./{caminho}` subdirectory and fails at resolve time with a non-self-\
         locating `No such file or directory` error far from the source caixa.lisp. The \
         lacre pipeline embeds the value verbatim in its per-dep content-address \
         `path:{caminho}` at caixa-resolver/src/resolve.rs:189, so the byte lands in the \
         BLAKE3 closure and rides into every shell-spawned subprocess (the resolver's \
         `git clone`, a future `feira tofu` shell-out, a future operator-side `nix` \
         spawn) as the canonical shell-metachar / glob-expansion surface every peer \
         single-token-shaped typed slot already closes. The peer `:entrada :paths` axis \
         rejects `*` and `?` via `is_gateway_api_http_path`'s eleven-byte RFC-3986-\
         reserved set. Express the path as a bare relative single-token like \
         \"../caixa-teia\" — the sibling-workspace directory name carries no shell-glob \
         / pathname-expansion semantic.",
        ch = *byte as char
    )]
    FonteCaminhoShellGlob {
        nome: String,
        caminho: String,
        byte: u8,
    },
    #[error(
        ":deps entry {nome:?} :fonte (:tipo path …) :caminho {caminho:?} has a trailing \
         `/` (the resolver's `Path::join` resolves `\"../caixa-teia\"` and \
         `\"../caixa-teia/\"` to the same directory, but the lacre pipeline embeds the \
         value verbatim in its per-dep content-address `path:{caminho}` at \
         caixa-resolver/src/resolve.rs:189, so two authors whose only difference is \
         shell tab-completion emit byte-divergent BLAKE3 closures for the same caixa — \
         defeating the THEORY.md §V.2 render-determinism contract via the trailing-\
         separator vector (the canonical paste-from-shell-tab-completion + paste-from-\
         `pwd`-with-`/`-suffix footgun, and the canonical Cargo-style `path = \
         \"../caixa-teia/\"` paste-from-Cargo-manifest cross-idiom leak). Drop the \
         trailing `/`; every `:caminho` value names a sibling-workspace directory \
         already, so the trailing separator carries no information. Use \
         `\"../caixa-teia\"` rather than `\"../caixa-teia/\"`)"
    )]
    FonteCaminhoTrailingSlash { nome: String, caminho: String },
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
    fn validate_rejects_git_fonte_with_repo_carrying_fragment_anchor() {
        // The fail-before-pass-after pin for the canonical paste-from-
        // browser-address-bar footgun on `:repo`: an author copies a
        // GitHub permalink to a README anchor / line-permalink and
        // forgets to trim the `#fragment` tail. Until this arm landed
        // `:repo "https://github.com/pleme-io/caixa-teia#readme"`
        // silently passed every prior arm (no whitespace, no control
        // chars, no non-ASCII, contains a `:`, doesn't start with `-`
        // or `:`), libcurl's URL parser stripped the `#readme` tail
        // before opening the HTTPS transport, and the lacre embedded
        // the value verbatim in its per-dep BLAKE3 closure — two
        // authors whose values differ only in their fragment anchor
        // (`#readme` vs `#L42`) resolve to the byte-identical upstream
        // `git clone` but lock to two distinct lacres, defeating the
        // THEORY.md §V.2 render-determinism contract. Same value-shape
        // axis-floor every peer typed surface enforces; peer `:fonte
        // :tag` / `:fonte :branch` already reject the byte-class through
        // `is_git_ref_name`'s alphabet (refs are leaf identifiers, no
        // URL grammar admitted) and `:entrada :paths` rejects `#` as
        // part of `is_gateway_api_http_path`'s RFC-3986-reserved set.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "https://github.com/pleme-io/caixa-teia#readme");
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm, got {reason:?}"
        );
        assert!(
            reason.contains("fragment"),
            "reason must name the URL fragment grammar, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_flake_ref_fragment() {
        // The symmetric paste-from-Nix-flake-ref footgun — an author
        // confuses the Nix flake-reference idiom (`github:foo/
        // bar#packageName`, where `#packageName` selects a flake
        // output) with the bare git `:repo` shape. The pleme-io
        // substrate authors compose flakes downstream of caixa
        // (caixa-flake renders a flake.nix), so the cross-idiom leak
        // is the canonical near-miss: the author writes the
        // flake-ref shape into a git `:repo` slot. Pinned separately
        // from the HTTPS-anchor arm so a future relaxation that
        // narrows to one URL scheme surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia#caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm, got {reason:?}"
        );
        assert!(
            reason.contains("Nix flake"),
            "reason must name the Nix-flake-ref cross-idiom footgun, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_query_string() {
        // The fail-before-pass-after pin for the canonical paste-from-
        // browser-address-bar footgun on `:repo` (peer with the
        // a68f818 fragment-`#` arm on the same axis). An author
        // copies a GitHub tab deep-link out of the address bar and
        // forgets to trim the `?tab=…` query tail. Until this arm
        // landed `:repo "https://github.com/pleme-io/caixa-teia?tab=readme-ov-file"`
        // silently passed every prior arm (no whitespace, no control
        // chars, no non-ASCII, no `#` fragment, contains a `:`,
        // doesn't start with `-` or `:`); GitHub silently ignored
        // the `?query` tail and served the same repo regardless;
        // the lacre embedded the value verbatim in its per-dep
        // BLAKE3 closure — two authors whose values differ only in
        // their query tail (`?tab=readme-ov-file` vs `?ref=main` vs
        // `?utm_source=twitter`) resolve to the byte-identical
        // upstream `git clone` but lock to two distinct lacres,
        // defeating the THEORY.md §V.2 render-determinism contract
        // on the same axis the `#` fragment arm closes. Same value-
        // shape axis-floor every peer typed surface enforces; peer
        // `:fonte :tag` / `:fonte :branch` already reject the byte-
        // class through `is_git_ref_name`'s alphabet (refspec glob
        // wildcards, caixa-core/src/render.rs:1426) and `:entrada
        // :paths` rejects `?` as the query separator in
        // `is_gateway_api_http_path` (caixa-core/src/render.rs:473).
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia?tab=readme-ov-file".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(
            repo,
            "https://github.com/pleme-io/caixa-teia?tab=readme-ov-file"
        );
        assert!(
            reason.contains("must not contain `?`"),
            "reason must surface the query-`?` arm, got {reason:?}"
        );
        assert!(
            reason.contains("query"),
            "reason must name the URL query grammar, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_utm_tracker() {
        // The symmetric paste-from-social-share footgun — an author
        // copies a repo URL out of a Slack unfurl / Twitter share /
        // newsletter link / Discord embed and forgets to trim the
        // `?utm_source=…` / `?utm_medium=…` / `?utm_campaign=…`
        // campaign-tracker tail. Every major social-share / unfurl /
        // newsletter platform appends these UTM parameters; the
        // canonical near-miss on the `:repo` axis. Pinned separately
        // from the GitHub-tab-deep-link arm so a future relaxation
        // that narrows to one query-parameter class surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia?utm_source=twitter&utm_campaign=launch"
                .into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `?`"),
            "reason must surface the query-`?` arm, got {reason:?}"
        );
        assert!(
            reason.contains("campaign-tracker"),
            "reason must name the campaign-tracker paste footgun, got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_query_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the query-`?` arm are
        // both per-byte arms inside the same `for &b in s.as_bytes()`
        // loop, so the byte that appears first in the value's byte
        // order wins. A `:repo "https://github.com/p/x#readme?ref=main"`
        // (fragment before query — unusual URL-grammar but value-
        // disjoint at byte level) carries both `#` and `?`; the `#`
        // byte appears first, so the fragment-`#` arm fires, surfacing
        // the more self-locating diagnostic on the byte the author
        // pasted earliest in the URL. Mirrors the peer cascade
        // discipline `fonte_repo_control_char_fires_before_fragment`
        // pins on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme?ref=main".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before query-`?` when \
             `#` byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_control_char_fires_before_fragment() {
        // Cascade pin: the control-char arm structurally precedes the
        // fragment-`#` arm. A value like `"github:p/x\n#readme"` probes
        // positive on both arms (contains LF and `#`), but the narrower
        // POSIX-syscall-rejected / CRLF-injection-class diagnostic
        // (`control character`) wins so the author sees the more
        // self-locating arm first. Mirrors the peer cascade discipline
        // every prior `:repo` byte-class arm establishes.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia\n#readme".into(),
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
    fn validate_rejects_git_fonte_with_repo_carrying_embedded_backslash() {
        // The fail-before-pass-after pin for the canonical Windows-
        // file-path-confusion footgun on `:repo` (peer with the 3a4e1d7
        // backslash arm on the sibling `:caminho` path-fonte axis).
        // An author pastes a Windows Explorer address-bar / PowerShell
        // `Get-Location` output into a `file://` URL slot, producing
        // `file:///C:\Users\me\caixa-teia`. Until this arm landed the
        // value silently passed every prior arm (no whitespace, no
        // control chars, no non-ASCII, no `#`, no `?`, doesn't start
        // with `-` or `:`); libcurl's URL parser silently translates
        // `\` → `/` on some platforms and refuses it on others, so
        // the byte rides verbatim into the lacre's per-dep content-
        // address but is silently rewritten / rejected at the wire —
        // two authors whose `:repo` values differ only in backslash-
        // vs-forward-slash (`file:///C:\path` vs `file:///C:/path`)
        // resolve to the byte-identical local clone but lock to two
        // distinct BLAKE3 closures, defeating the THEORY.md §V.2
        // render-determinism contract on the same axis the `#`
        // fragment and `?` query arms close. Same value-shape axis-
        // floor every peer typed surface enforces; the `:caminho`
        // 3a4e1d7 arm closes the same byte on the path-fonte axis.
        let d = dep_with_fonte(DepSource::Git {
            repo: "file:///C:\\Users\\me\\caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "file:///C:\\Users\\me\\caixa-teia");
        assert!(
            reason.contains("must not contain `\\`"),
            "reason must surface the backslash-`\\` arm, got {reason:?}"
        );
        assert!(
            reason.contains("Windows"),
            "reason must name the Windows-path-confusion footgun, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_mangled_https_backslashes() {
        // The symmetric Win32-shell-mangled-slashes footgun — an author
        // copies `https://github.com/foo/bar` into a Win32 shell that
        // rewrites every `/` to `\` (the canonical `cmd.exe` path-
        // separator-coercion bug), pastes the result into a `:repo`
        // slot, and produces `https:\\github.com\foo\bar`. Pinned
        // separately from the `file://` Explorer-paste arm so a future
        // relaxation that narrows to one URL scheme surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https:\\\\github.com\\pleme-io\\caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `\\`"),
            "reason must surface the backslash-`\\` arm, got {reason:?}"
        );
        assert!(
            reason.contains("path separator") || reason.contains("path-segment separator"),
            "reason must name the URL path-segment separator grammar, got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_backslash_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the backslash-`\` arm
        // are both per-byte arms inside the same `for &b in s.as_bytes()`
        // loop, so the byte that appears first in the value's byte order
        // wins. A `:repo "https://github.com/p/x#readme\\foo"` carries
        // both `#` and `\`; the `#` byte appears first, so the fragment-
        // `#` arm fires, surfacing the more self-locating diagnostic on
        // the byte the author pasted earliest in the URL. Mirrors the
        // peer cascade discipline `fonte_repo_fragment_fires_before_query_when_fragment_first`
        // pins on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme\\foo".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before backslash-`\\` when \
             `#` byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_uri_template_placeholder() {
        // The fail-before-pass-after pin for the canonical URI Template
        // (RFC 6570) placeholder footgun on `:repo`. An author copies a
        // README quick-start snippet / OpenAPI `servers:` URL / Helm
        // chart `home:` template that carries unresolved
        // `{org}` / `{repo}` placeholders and pastes the raw template
        // into the `:repo` slot, expecting the substrate to resolve the
        // placeholder downstream. Until this arm landed the value
        // silently passed every prior arm (no whitespace, no control
        // chars, no non-ASCII, no `#`, no `?`, no `\`, doesn't start
        // with `-` or `:`); libcurl percent-encodes `{` / `}` to `%7B`
        // / `%7D` on the wire, so the byte rides verbatim into the
        // lacre's per-dep content-address but round-trips inconsistently
        // between the lacre's per-dep content-address and the
        // resolver's `git clone <repo>` invocation, defeating the
        // THEORY.md §V.2 render-determinism contract on the same axis
        // the `#` fragment, `?` query, and `\` backslash arms close;
        // every git porcelain entry-point additionally fetches a
        // nonexistent literal-`{placeholder}`-named path far from the
        // source caixa.lisp.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/{org}/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "https://github.com/{org}/caixa-teia");
        assert!(
            reason.contains("must not contain `{`"),
            "reason must surface the open-brace `{{` arm, got {reason:?}"
        );
        assert!(
            reason.contains("URI Template") || reason.contains("RFC 6570"),
            "reason must name the RFC 6570 URI Template grammar, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_handlebars_doubled_brace() {
        // The symmetric Mustache / Handlebars doubled-brace
        // substitution-form footgun every CI / IaC templating engine
        // (Argo Workflows, Jinja2, Liquid, Vue / Angular interpolation,
        // GitHub Actions `${{ … }}` even though Actions uses `${{`) /
        // chart README quick-start snippet emits. Pinned separately
        // from the single-`{` `{org}` arm so a future relaxation that
        // narrows to one substitution-form surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/{{org}}/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `{`"),
            "reason must surface the open-brace `{{` arm, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_closing_brace_only() {
        // Asymmetric `}`-only shape — covers the closing-brace-by-
        // itself footgun (an author truncated `{org}/{repo}` mid-edit
        // and left a trailing `}` from the prior template fragment,
        // or pasted a value that included a closing brace from a
        // surrounding shell context). Pinned to ensure the predicate
        // refuses each brace independently rather than only when both
        // appear — a future regression that ANDs the two byte tests
        // surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia}".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `}`"),
            "reason must surface the close-brace `}}` arm, got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_template_placeholder_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the template-`{` /
        // `}` arm are both per-byte arms inside the same
        // `for &b in s.as_bytes()` loop, so the byte that appears
        // first in the value's byte order wins. A `:repo
        // "https://github.com/p/x#readme{org}"` carries both `#` and
        // `{`; the `#` byte appears first, so the fragment-`#` arm
        // fires, surfacing the more self-locating diagnostic on the
        // byte the author pasted earliest in the URL. Mirrors the
        // peer cascade discipline `fonte_repo_fragment_fires_before_backslash_when_fragment_first`
        // pins on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme{org}".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before template-`{{` when \
             `#` byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_output_redirection() {
        // The fail-before-pass-after pin for the canonical
        // shell-output-redirection footgun on `:repo`: an author
        // pastes a shell-pipeline tail (`git clone <repo> > build.log`
        // / `… >output.txt`) into the `:repo` slot without trimming
        // the redirect. Until this arm landed the value silently
        // passed every prior arm (no whitespace, no control chars,
        // no non-ASCII, no `#`, no `?`, no `\`, no `{`/`}`, doesn't
        // start with `-` or `:`); RFC 3986 §2 lists `<` / `>` in the
        // 'delims' / 'unwise' set and the WHATWG URL spec's fragment
        // percent-encode set maps `>` → `%3E` on the wire, so the
        // byte rides verbatim into the lacre's per-dep BLAKE3 closure
        // but is silently rewritten or rejected at libcurl's URL-
        // parser layer — two authors whose values differ only in
        // their redirect tail (`>build.log` vs nothing) resolve to
        // the byte-identical upstream `git clone` but lock to two
        // distinct lacres, defeating the THEORY.md §V.2 render-
        // determinism contract. Peer with the `:caminho` axis's
        // `FonteCaminhoShellRedirection` arm (e457141) on the sibling
        // path-fonte axis, and `is_gateway_api_http_path`'s eleven-
        // byte RFC-3986-reserved set on `:entrada :paths`.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia>build.log".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "https://github.com/pleme-io/caixa-teia>build.log");
        assert!(
            reason.contains("must not contain `>`"),
            "reason must surface the output-redirection `>` arm, got {reason:?}"
        );
        assert!(
            reason.contains("redirection") || reason.contains("'delims'"),
            "reason must name the shell-redirection / RFC-3986-unwise rationale, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_input_redirection() {
        // The symmetric shell-input-redirection footgun — an author
        // pastes a shell-pipeline head (`git clone <input.url` /
        // `cat <README.md`) into the `:repo` slot. Pinned separately
        // from the `>`-output arm so a future relaxation that only
        // catches one of the two redirect bytes surfaces here. Peer
        // with the `:caminho` axis's `FonteCaminhoShellRedirection`
        // arm which closes both `<` and `>` under the same banner.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia<input.url".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `<`"),
            "reason must surface the input-redirection `<` arm, got {reason:?}"
        );
        assert!(
            reason.contains("RFC 3986") || reason.contains("'unwise'"),
            "reason must name the RFC 3986 'unwise' grammar, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_backtick_command_substitution() {
        // The fail-before-pass-after pin for the canonical
        // paste-from-shell-prompt-with-backticked-substitution footgun
        // on `:repo` (peer with the c4d62b3 backtick arm on the sibling
        // `:caminho` path-fonte axis). An author pastes a URL whose
        // segment carries a backticked command-substitution wrapper
        // (`` `whoami` ``, `` `git config user.name` ``, `` `pwd` ``)
        // from a doc / README quick-start snippet that expected the
        // substrate to substitute the value downstream. Until this arm
        // landed the value silently passed every prior arm (no
        // whitespace, no control chars, no non-ASCII, no `#`, no `?`,
        // no `\`, no `{`/`}`, no `<`/`>`, doesn't start with `-` or
        // `:`); RFC 3986 §2 lists the backtick byte in the 'delims' /
        // 'unwise' set and the WHATWG URL spec's fragment percent-
        // encode set maps `` ` `` → `%60` on the wire, so the byte
        // rides verbatim into the lacre's per-dep BLAKE3 closure but
        // is silently rewritten or rejected at libcurl's URL-parser
        // layer — two authors whose values differ only in their
        // backtick wrapper (`` `whoami` `` vs nothing) resolve to the
        // byte-identical upstream `git clone` but lock to two distinct
        // lacres, defeating the THEORY.md §V.2 render-determinism
        // contract. Peer with the `:caminho` axis's
        // `FonteCaminhoShellCommandSubstitution` arm (c4d62b3) on the
        // sibling path-fonte axis, and `is_gateway_api_http_path`'s
        // eleven-byte RFC-3986-reserved set on `:entrada :paths`.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/`whoami`/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "https://github.com/pleme-io/`whoami`/caixa-teia");
        assert!(
            reason.contains("must not contain `` ` ``"),
            "reason must surface the backtick command-substitution arm, got {reason:?}"
        );
        assert!(
            reason.contains("command-substitution") || reason.contains("'unwise'"),
            "reason must name the shell-command-substitution / RFC-3986-unwise rationale, \
             got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_backtick_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the backtick command-
        // substitution arm are both per-byte arms inside the same
        // `for &b in s.as_bytes()` loop, so the byte that appears first
        // in the value's byte order wins. A `:repo
        // "https://github.com/p/x#readme/`whoami`"` carries both `#`
        // and backtick; the `#` byte appears first, so the fragment-
        // `#` arm fires, surfacing the more self-locating diagnostic
        // on the byte the author pasted earliest in the URL. Mirrors
        // the peer cascade discipline
        // `fonte_repo_fragment_fires_before_shell_redirection_when_fragment_first`
        // pins on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme/`whoami`".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before backtick when `#` byte \
             appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_shell_redirection_fires_before_backtick_when_redirection_first() {
        // Cascade pin: the shell-redirection `<` / `>` arm and the
        // backtick command-substitution arm are both per-byte arms
        // inside the same `for &b in s.as_bytes()` loop, so the byte
        // that appears first in the value's byte order wins. A `:repo
        // "https://github.com/p/x>build.log/`whoami`"` carries both
        // `>` and backtick; the `>` byte appears first, so the
        // shell-redirection arm fires, surfacing the more self-
        // locating diagnostic on the byte the author pasted earliest
        // in the URL. Pins the natural-order cascade so a future
        // reorder of the per-byte arms surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia>build.log/`whoami`".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `>`"),
            "reason must surface the shell-redirection `>` arm (fires before backtick when \
             `>` byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_shell_redirection_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the shell-redirection
        // `<` / `>` arm are both per-byte arms inside the same
        // `for &b in s.as_bytes()` loop, so the byte that appears
        // first in the value's byte order wins. A `:repo
        // "https://github.com/p/x#readme>build.log"` carries both
        // `#` and `>`; the `#` byte appears first, so the fragment-
        // `#` arm fires, surfacing the more self-locating diagnostic
        // on the byte the author pasted earliest in the URL. Mirrors
        // the peer cascade discipline
        // `fonte_repo_fragment_fires_before_template_placeholder_when_fragment_first`
        // pins on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme>build.log".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before shell-redirection `>` when \
             `#` byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_shell_pipe() {
        // The fail-before-pass-after pin for the canonical
        // paste-from-shell-prompt-with-piped-pipeline footgun on
        // `:repo` (peer with the 124106f pipe arm on the sibling
        // `:caminho` path-fonte axis). An author pastes a shell
        // pipeline (`git clone <url> | tee build.log`,
        // `git ls-remote <url> | head`) into the `:repo` slot,
        // forgetting to trim the `| <consumer>` tail. Until this arm
        // landed the value silently passed every prior arm (no
        // whitespace, no control chars, no non-ASCII, no `#`, no `?`,
        // no `\`, no `{`/`}`, no `<`/`>`, no `` ` ``, doesn't start
        // with `-` or `:`); RFC 3986 §2 lists the pipe byte in the
        // 'unwise' set and the WHATWG URL spec's fragment percent-
        // encode set maps `|` → `%7C` on the wire, so the byte rides
        // verbatim into the lacre's per-dep BLAKE3 closure but is
        // silently rewritten or rejected at libcurl's URL-parser
        // layer — two authors whose values differ only in their pipe
        // tail (`|tee build.log` vs nothing) resolve to the byte-
        // identical upstream `git clone` but lock to two distinct
        // lacres, defeating the THEORY.md §V.2 render-determinism
        // contract. Peer with the `:caminho` axis's
        // `FonteCaminhoShellPipe` arm (124106f) on the sibling path-
        // fonte axis, and `is_gateway_api_http_path`'s eleven-byte
        // RFC-3986-reserved set on `:entrada :paths`.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia|tee build.log".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "https://github.com/pleme-io/caixa-teia|tee build.log");
        assert!(
            reason.contains("must not contain `|`"),
            "reason must surface the shell-pipe arm, got {reason:?}"
        );
        assert!(
            reason.contains("pipe") || reason.contains("'unwise'"),
            "reason must name the shell-pipe / RFC-3986-unwise rationale, got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_pipe_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the pipe arm are both
        // per-byte arms inside the same `for &b in s.as_bytes()` loop,
        // so the byte that appears first in the value's byte order
        // wins. A `:repo "https://github.com/p/x#readme|tee"` carries
        // both `#` and `|`; the `#` byte appears first, so the
        // fragment-`#` arm fires, surfacing the more self-locating
        // diagnostic on the byte the author pasted earliest in the
        // URL. Mirrors the peer cascade discipline
        // `fonte_repo_fragment_fires_before_backtick_when_fragment_first`
        // pins on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme|tee".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before pipe when `#` byte \
             appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_backtick_fires_before_pipe_when_backtick_first() {
        // Cascade pin: the backtick arm and the pipe arm are both per-
        // byte arms inside the same `for &b in s.as_bytes()` loop, so
        // the byte that appears first in the value's byte order wins.
        // A `:repo "https://github.com/p/x/`whoami`|tee"` carries both
        // `` ` `` and `|`; the backtick byte appears first, so the
        // backtick arm fires, surfacing the more self-locating
        // diagnostic on the byte the author pasted earliest in the
        // URL. Pins the natural-order cascade so a future reorder of
        // the per-byte arms surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia/`whoami`|tee".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `` ` ``"),
            "reason must surface the backtick arm (fires before pipe when `` ` `` byte \
             appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_shell_command_separator() {
        // The fail-before-pass-after pin for the canonical
        // paste-from-shell-prompt-with-sequential-command-tail footgun
        // on `:repo` (peer with the 05c358e `;` arm on the sibling
        // `:caminho` path-fonte axis). An author pastes a shell
        // one-liner that chained a cleanup tail after the URL
        // (`git clone <url>; rm -rf build`, `git ls-remote <url>;
        // echo done`) into the `:repo` slot, forgetting to trim the
        // `; <cmd>` tail. Until this arm landed the value silently
        // passed every prior `is_git_repo_url` arm (no whitespace, no
        // control chars, no non-ASCII, no `#`, no `?`, no `\`, no
        // `{`/`}`, no `<`/`>`, no `` ` ``, no `|`, doesn't start with
        // `-` or `:`); RFC 3986 §2 lists `;` in the 'sub-delims' /
        // reserved set and the WHATWG URL spec's fragment percent-
        // encode set maps `;` → `%3B` on the wire, so the byte rides
        // verbatim into the lacre's per-dep BLAKE3 closure but is
        // silently rewritten at libcurl's URL-parser layer — two
        // authors whose values differ only in their sequential-command
        // tail (`; rm -rf build` vs nothing) resolve to the byte-
        // identical upstream `git clone` but lock to two distinct
        // lacres, defeating the THEORY.md §V.2 render-determinism
        // contract. Peer with the `:caminho` axis's
        // `FonteCaminhoShellSemicolon` arm (05c358e) on the sibling
        // path-fonte axis, and `is_gateway_api_http_path`'s eleven-
        // byte RFC-3986-reserved set on `:entrada :paths`.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia; rm -rf build".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "https://github.com/pleme-io/caixa-teia; rm -rf build");
        assert!(
            reason.contains("must not contain `;`"),
            "reason must surface the shell-command-separator arm, got {reason:?}"
        );
        assert!(
            reason.contains("sequential-command") || reason.contains("'sub-delims'"),
            "reason must name the shell-command-separator / RFC-3986-sub-delims \
             rationale, got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_semicolon_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the semicolon arm are
        // both per-byte arms inside the same `for &b in s.as_bytes()`
        // loop, so the byte that appears first in the value's byte
        // order wins. A `:repo "https://github.com/p/x#readme; rm"`
        // carries both `#` and `;`; the `#` byte appears first, so the
        // fragment-`#` arm fires, surfacing the more self-locating
        // diagnostic on the byte the author pasted earliest in the URL.
        // Mirrors the peer cascade discipline
        // `fonte_repo_fragment_fires_before_pipe_when_fragment_first`
        // pins on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme; rm".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before semicolon when `#` \
             byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_pipe_fires_before_semicolon_when_pipe_first() {
        // Cascade pin: the pipe arm and the semicolon arm are both
        // per-byte arms inside the same `for &b in s.as_bytes()` loop,
        // so the byte that appears first in the value's byte order
        // wins. A `:repo "https://github.com/p/x|tee; rm"` carries
        // both `|` and `;`; the `|` byte appears first, so the
        // pipe arm fires, surfacing the more self-locating diagnostic
        // on the byte the author pasted earliest in the URL. Pins the
        // natural-order cascade so a future reorder of the per-byte
        // arms surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia|tee; rm".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `|`"),
            "reason must surface the pipe arm (fires before semicolon when `|` byte \
             appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_shell_background() {
        // The fail-before-pass-after pin for the canonical
        // paste-from-shell-prompt-with-background-launch-tail footgun
        // on `:repo` (peer with the e12e4f3 `&` arm on the sibling
        // `:caminho` path-fonte axis). An author pastes a shell one-
        // liner that detached the clone into the background
        // (`git clone <url> & sleep 1`, `git clone <url> && cd …`)
        // into the `:repo` slot, forgetting to trim the `& <cmd>` /
        // `&& <cmd>` tail. Until this arm landed the value silently
        // passed every prior `is_git_repo_url` arm (no whitespace,
        // no control chars, no non-ASCII, no `#`, no `?`, no `\`,
        // no `{`/`}`, no `<`/`>`, no `` ` ``, no `|`, no `;`,
        // doesn't start with `-` or `:`); RFC 3986 §2 lists `&` in
        // the 'sub-delims' / reserved set and the WHATWG URL spec's
        // fragment percent-encode set maps `&` → `%26` on the wire,
        // so the byte rides verbatim into the lacre's per-dep
        // BLAKE3 closure but is silently rewritten at libcurl's
        // URL-parser layer — two authors whose values differ only
        // in their background-launch tail (`& sleep 1` vs nothing)
        // resolve to the byte-identical upstream `git clone` but
        // lock to two distinct lacres, defeating the THEORY.md
        // §V.2 render-determinism contract. Peer with the
        // `:caminho` axis's `FonteCaminhoShellBackground` arm
        // (e12e4f3) on the sibling path-fonte axis, and
        // `is_gateway_api_http_path`'s eleven-byte RFC-3986-
        // reserved set on `:entrada :paths`.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia&sleep".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "https://github.com/pleme-io/caixa-teia&sleep");
        assert!(
            reason.contains("must not contain `&`"),
            "reason must surface the shell-background / logical-AND arm, got {reason:?}"
        );
        assert!(
            reason.contains("background-task") || reason.contains("'sub-delims'"),
            "reason must name the shell-background / RFC-3986-sub-delims rationale, \
             got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_logical_and() {
        // The fail-before-pass-after pin for the symmetric `&&`
        // logical-AND build-chain paste footgun: an author pastes
        // a `git clone <url> && cd <repo>` build-chain one-liner
        // and forgets to trim the `&& <cmd>` tail. The `&&` shape
        // is the same `&` byte twice in a row; the per-byte arm
        // fires on the first `&` it sees. Pinned separately from
        // the single-`&` background-launch shape so a future
        // diagnostic-surface change that special-cased the
        // doubled-byte form surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "github:pleme-io/caixa-teia&&echo".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `&`"),
            "reason must surface the shell-background / logical-AND arm on the doubled-`&&` \
             shape too, got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_background_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the background-`&`
        // arm are both per-byte arms inside the same `for &b in
        // s.as_bytes()` loop, so the byte that appears first in the
        // value's byte order wins. A `:repo
        // "https://github.com/p/x#readme & sleep"` carries both `#`
        // and `&`; the `#` byte appears first, so the fragment-`#`
        // arm fires, surfacing the more self-locating diagnostic on
        // the byte the author pasted earliest in the URL. Mirrors
        // the peer cascade discipline
        // `fonte_repo_fragment_fires_before_semicolon_when_fragment_first`
        // on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme&sleep".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before background-`&` when `#` \
             byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_semicolon_fires_before_background_when_semicolon_first() {
        // Cascade pin: the semicolon arm and the background-`&` arm
        // are both per-byte arms inside the same `for &b in
        // s.as_bytes()` loop, so the byte that appears first in the
        // value's byte order wins. A `:repo
        // "https://github.com/p/x; rm & sleep"` carries both `;` and
        // `&`; the `;` byte appears first, so the semicolon arm
        // fires, surfacing the more self-locating diagnostic on the
        // byte the author pasted earliest in the URL. Pins the
        // natural-order cascade so a future reorder of the per-byte
        // arms surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia;rm&sleep".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `;`"),
            "reason must surface the semicolon arm (fires before background-`&` when `;` \
             byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_shell_variable_expansion() {
        // The fail-before-pass-after pin for the canonical
        // paste-from-shell-prompt-with-unsubstituted-variable footgun
        // on `:repo` (peer with the f4efe9c `$` arm on the sibling
        // `:caminho` path-fonte axis). An author pastes a shell one-
        // liner that referenced an environment variable
        // (`git clone https://github.com/$ORG/x`, `git clone
        // github:$USER/repo`) into the `:repo` slot, forgetting to
        // substitute the literal value at author time. Until this arm
        // landed the value silently passed every prior
        // `is_git_repo_url` arm (no whitespace, no control chars, no
        // non-ASCII, no `#`, no `?`, no `\`, no `{`/`}`, no `<`/`>`,
        // no `` ` ``, no `|`, no `;`, no `&`, doesn't start with `-`
        // or `:`); RFC 3986 §2 lists `$` in the 'sub-delims' /
        // reserved set and the WHATWG URL spec's fragment percent-
        // encode set maps `$` → `%24` on the wire, so the byte rides
        // verbatim into the lacre's per-dep BLAKE3 closure but is
        // silently rewritten at libcurl's URL-parser layer — two
        // authors whose values differ only in their `$VAR` /
        // `${VAR}` / `$(cmd)` expansion tail resolve to the byte-
        // identical upstream `git clone` but lock to two distinct
        // lacres, defeating the THEORY.md §V.2 render-determinism
        // contract. Beyond determinism, the value is a structural
        // host-layout leak: two authors with the same `:repo` slot
        // but different `$ORG` / `$HOME` / `$WORKSPACE` resolve
        // different upstreams. Peer with the `:caminho` axis's
        // `FonteCaminhoVarExpansion` arm (f4efe9c) on the sibling
        // path-fonte axis, and `is_gateway_api_http_path`'s eleven-
        // byte RFC-3986-reserved set on `:entrada :paths`.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/$ORG/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { nome, repo, reason } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(repo, "https://github.com/$ORG/caixa-teia");
        assert!(
            reason.contains("must not contain `$`"),
            "reason must surface the shell-variable-expansion arm, got {reason:?}"
        );
        assert!(
            reason.contains("variable-expansion") || reason.contains("'sub-delims'"),
            "reason must name the shell-variable-expansion / RFC-3986-sub-delims \
             rationale, got {reason:?}"
        );
    }

    #[test]
    fn validate_rejects_git_fonte_with_repo_carrying_braced_variable_expansion() {
        // The fail-before-pass-after pin for the symmetric POSIX-
        // shell braced `${VAR}` expansion paste footgun: an author
        // pastes a CI-manifest line `git clone
        // https://github.com/${WORKSPACE}/x` (the canonical GitHub
        // Actions / GitLab CI / Drone shape) and forgets to
        // substitute the literal value. The `${...}` shape is the
        // same `$` byte at the leading position of the expansion;
        // the per-byte arm fires on the `$`. Pinned separately from
        // the bare-`$VAR` shape so a future diagnostic-surface
        // change that special-cased the braced form surfaces here.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/${WORKSPACE}/caixa-teia".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `$`"),
            "reason must surface the shell-variable-expansion arm on the braced `${{...}}` \
             shape too, got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_fragment_fires_before_var_expansion_when_fragment_first() {
        // Cascade pin: the fragment-`#` arm and the var-expansion-`$`
        // arm are both per-byte arms inside the same `for &b in
        // s.as_bytes()` loop, so the byte that appears first in the
        // value's byte order wins. A `:repo
        // "https://github.com/p/x#readme$HOME"` carries both `#` and
        // `$`; the `#` byte appears first, so the fragment-`#` arm
        // fires, surfacing the more self-locating diagnostic on the
        // byte the author pasted earliest in the URL. Mirrors the
        // peer cascade discipline
        // `fonte_repo_fragment_fires_before_background_when_fragment_first`
        // on the prior `:repo` byte-class arm.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia#readme$HOME".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `#`"),
            "reason must surface the fragment-`#` arm (fires before var-expansion-`$` when \
             `#` byte appears first in value), got {reason:?}"
        );
    }

    #[test]
    fn fonte_repo_background_fires_before_var_expansion_when_background_first() {
        // Cascade pin: the background-`&` arm and the
        // var-expansion-`$` arm are both per-byte arms inside the
        // same `for &b in s.as_bytes()` loop, so the byte that
        // appears first in the value's byte order wins. A `:repo
        // "https://github.com/p/x&sleep$HOME"` carries both `&` and
        // `$`; the `&` byte appears first, so the background arm
        // fires, surfacing the more self-locating diagnostic on the
        // byte the author pasted earliest in the URL. Pins the
        // natural-order cascade so a future reorder of the per-byte
        // arms surfaces here — `$` is the most recent byte-class arm,
        // so the cascade-pin sweep extends to cover every immediately
        // prior byte arm (`#`, `&`) firing first when ordered ahead
        // of `$` in the value.
        let d = dep_with_fonte(DepSource::Git {
            repo: "https://github.com/pleme-io/caixa-teia&sleep$HOME".into(),
            tag: Some("v0.1.0".into()),
            rev: None,
            branch: None,
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteRepoShape { reason, .. } = err else {
            panic!("expected FonteRepoShape, got other variant");
        };
        assert!(
            reason.contains("must not contain `&`"),
            "reason must surface the background-`&` arm (fires before var-expansion-`$` when \
             `&` byte appears first in value), got {reason:?}"
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
    fn validate_rejects_path_fonte_with_absolute_caminho() {
        // The fail-before-pass-after pin for the absolute-`:caminho`
        // shape: `(:tipo path :caminho "/home/me/work/caixa-teia")`.
        // Until this gate landed an absolute `:caminho` silently
        // passed validate; the lacre pipeline embedded the
        // host-specific filesystem path verbatim in its
        // content-address (`conteudo: format!("path:{caminho}")`,
        // caixa-resolver/src/resolve.rs:189), so the BLAKE3 closure
        // differed per machine — the build succeeded but two CI
        // runners with different `${HOME}` layouts emitted two
        // distinct lacres for the byte-identical caixa, silently
        // breaking the THEORY.md §V.2 render-determinism contract
        // far from the source caixa.lisp. The new gate moves the
        // check to validate time and names the offending dep +
        // caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/home/me/work/caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoAbsolute { nome, caminho } = err else {
            panic!("expected FonteCaminhoAbsolute, got other variant");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "/home/me/work/caixa-teia");
    }

    #[test]
    fn validate_accepts_path_fonte_with_parent_escape_caminho() {
        // The canonical sibling-workspace dep form
        // (`:caminho "../caixa-teia"`) remains accepted. The
        // absolute-path gate above is specifically narrower than the
        // shared [`crate::render::is_sandboxed_relative_path`]
        // predicate (which additionally forbids `..` traversal): a
        // local-path dep's canonical author surface is the in-tree
        // sibling-workspace path, so a full sandboxed-relative-path
        // lift would structurally reject every legitimate path-fonte
        // dep. Pinned so a future tightening to the full predicate
        // surfaces here as a structural decision, not a silent break.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_accepts_path_fonte_with_deeply_nested_relative_caminho() {
        // A multi-segment relative `:caminho`
        // (`"vendor/forks/caixa-teia"`) remains accepted — the
        // absolute-path gate brackets the host-layout-leaking shape
        // at the leading-`/` boundary only; every relative shape past
        // the empty arm continues to pass. Pinned alongside the
        // `..`-traversal positive control so a future tightening
        // surfaces the full set of legitimate relative forms here
        // rather than at a downstream consumer.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "vendor/forks/caixa-teia".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_rejects_path_fonte_with_tilde_prefix_caminho() {
        // The fail-before-pass-after pin for the tilde-expansion
        // `:caminho` shape: `(:tipo path :caminho "~/work/caixa-teia")`.
        // Until this gate landed the b94fd83 absolute arm let `~/foo`
        // through (`Path::is_absolute` returns false on a leading `~`
        // — the tilde is a shell-expansion convention, not a POSIX
        // path component), so the lacre embedded the value verbatim
        // and the resolver folded it through `Path::join` without
        // expansion, looking for a literal `./~/work/caixa-teia`
        // subdirectory and failing at resolve time with a
        // `No such file or directory` error far from the source
        // caixa.lisp. The new gate moves the check to validate time
        // and names the offending dep + caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "~/work/caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoTildeExpansion { nome, caminho } = err else {
            panic!("expected FonteCaminhoTildeExpansion, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "~/work/caixa-teia");
    }

    #[test]
    fn validate_rejects_path_fonte_with_bare_tilde_caminho() {
        // The bare `~` form (canonical "I meant `$HOME` and forgot
        // the rest"): both the leading-tilde arm catches it and the
        // canonical-user-tilde shell idiom (`~alice/dev/caixa-teia`)
        // sweeps through the same arm. Pinned both to ensure the
        // gate doesn't narrow to `~/` only.
        for s in ["~", "~alice/dev/caixa-teia", "~/", "~root/work"] {
            let d = dep_with_fonte(DepSource::Path { caminho: s.into() });
            let err = d.validate().unwrap_err();
            assert!(
                matches!(err, DepError::FonteCaminhoTildeExpansion { .. }),
                "{s:?} → {err:?}",
            );
        }
    }

    #[test]
    fn validate_accepts_path_fonte_with_mid_path_tilde_caminho() {
        // The leading-`~` is the canonical shell-expansion footgun —
        // a tilde mid-path (`"../foo~bar/caixa-teia"` — the canonical
        // backup-file-suffix idiom) is a legitimate POSIX path byte
        // with no shell-expansion semantic at the leading position.
        // Pinned so the gate doesn't widen to a full no-tilde-anywhere
        // sweep that would break every legitimate-shape backup-file
        // path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo~bar/caixa-teia".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_empty_fires_before_tilde_expansion() {
        // Cascade pin: the empty arm structurally precedes the
        // tilde arm (the bytes `""` and `"~"` don't overlap), but the
        // pin establishes the precedence at the diagnostic-shape
        // level should a future codec round-trip ever produce a
        // probe-as-both value. Mirrors the peer
        // `fonte_repo_empty_fires_before_pin_missing` cascade
        // discipline.
        let d = dep_with_fonte(DepSource::Path {
            caminho: String::new(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoEmpty { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_tilde_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with
        // `validate_rejects_path_fonte_with_absolute_caminho`'s
        // payload assertion): the error's Display surfaces both the
        // offending `:nome` and the offending `:caminho` verbatim
        // so a `feira lint` run can render the diagnostic without
        // re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "~alice/dev/caixa-teia".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("~alice/dev/caixa-teia"),
            "diagnostic must quote the offending caminho: {rendered}",
        );
        assert!(
            rendered.contains('~'),
            "diagnostic must reference the tilde footgun: {rendered}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_dollar_prefix_caminho() {
        // The fail-before-pass-after pin for the shell-variable-
        // expansion `:caminho` shape: `(:tipo path :caminho
        // "$HOME/work/caixa-teia")`. Until this gate landed the
        // b94fd83 absolute arm + the a5c248e tilde arm both let
        // `$HOME/foo` through (`Path::is_absolute` returns false on
        // a leading `$` — the `$` is a shell convention, not a POSIX
        // path component; `starts_with('~')` returns false too), so
        // the lacre embedded the value verbatim and the resolver
        // folded it through `Path::join` without `$`-expansion,
        // looking for a literal `./$HOME/work/caixa-teia`
        // subdirectory and failing at resolve time with a
        // `No such file or directory` error far from the source
        // caixa.lisp. The new gate moves the check to validate time
        // and names the offending dep + caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "$HOME/work/caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoVarExpansion { nome, caminho } = err else {
            panic!("expected FonteCaminhoVarExpansion, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "$HOME/work/caixa-teia");
    }

    #[test]
    fn validate_rejects_path_fonte_with_dollar_brace_prefix_caminho() {
        // Sweep over every leading-`$` shape: the `${VAR}`-braced
        // form (canonical "paste-from-CI-manifest" footgun every
        // GitHub Actions / GitLab CI / Drone manifest carries on
        // `${WORKSPACE}`), the XDG idiom (`$XDG_CONFIG_HOME/caixa`,
        // canonical "I'm referencing a per-user config dir"),
        // and the bare `$` (canonical "I meant `$HOME` and forgot
        // the rest"). All shapes route through the same gate's
        // byte check. Pinned so the gate doesn't narrow to a
        // single shape (e.g. `$HOME/` only).
        for s in [
            "${HOME}/work/caixa-teia",
            "${WORKSPACE}/caixa-teia",
            "$XDG_CONFIG_HOME/caixa",
            "$",
        ] {
            let d = dep_with_fonte(DepSource::Path { caminho: s.into() });
            let err = d.validate().unwrap_err();
            assert!(
                matches!(err, DepError::FonteCaminhoVarExpansion { .. }),
                "{s:?} → {err:?}",
            );
        }
    }

    #[test]
    fn validate_accepts_path_fonte_with_mid_path_dollar_caminho() {
        // The leading-`$` is the canonical shell-variable-expansion
        // footgun — a `$` mid-path (`"../foo$bar/caixa-teia"` — the
        // canonical "I have a file with `$` in its name" idiom; `$`
        // is a valid POSIX filename byte) is a legitimate path with
        // no shell-expansion semantic at the non-leading position.
        // Pinned so the gate doesn't widen to a full no-dollar-
        // anywhere sweep that would break every legitimate-shape
        // dollar-in-filename path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo$bar/caixa-teia".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_tilde_fires_before_var_expansion() {
        // Cascade pin: the tilde arm structurally precedes the var
        // arm (the bytes `~` and `$` don't overlap at the leading
        // position), but the pin establishes the precedence at the
        // diagnostic-shape level should a future codec round-trip
        // ever produce a probe-as-both value. Mirrors the peer
        // `fonte_caminho_empty_fires_before_tilde_expansion` cascade
        // discipline on the immediate-predecessor arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "~/work/caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoTildeExpansion { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_var_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_tilde_diagnostic_carries_offending_dep_and_caminho`'s
        // payload assertion on the immediate-predecessor arm): the
        // error's Display surfaces both the offending `:nome` and
        // the offending `:caminho` verbatim plus the `$` footgun
        // character itself so a `feira lint` run can render the
        // diagnostic without re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "${WORKSPACE}/caixa-teia".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("${WORKSPACE}/caixa-teia"),
            "diagnostic must quote the offending caminho: {rendered}",
        );
        assert!(
            rendered.contains('$'),
            "diagnostic must reference the dollar footgun: {rendered}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_embedded_nul() {
        // The fail-before-pass-after pin for the load-bearing NUL byte:
        // POSIX paths cannot contain `0x00` (every `std::fs` syscall
        // routes the path through `CString::new` which fails with
        // `NulError`); until this gate landed a `:caminho
        // "../caixa\0teia"` silently passed validate, the lacre
        // pipeline embedded the value verbatim, and the failure
        // surfaced at the resolver's `Path::join` → `CString::new`
        // boundary with a non-self-locating `NulError` far from the
        // source caixa.lisp. The new gate moves the check to validate
        // time and names the offending dep + caminho + offending byte
        // verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa\0teia".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoControlChar {
            nome,
            caminho,
            byte,
        } = err
        else {
            panic!("expected FonteCaminhoControlChar, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "../caixa\0teia");
        assert_eq!(byte, 0x00);
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_embedded_newline() {
        // The canonical paste-from-multiline-doc footgun on `:caminho`
        // — author copies `"../caixa-teia\n"` (trailing newline) out
        // of a multi-line code-fence or, worse, a `:caminho
        // "../caixa-teia\nrm -rf /"` value (CRLF-at-subprocess-argument
        // injection sibling on the path axis the `is_git_repo_url`
        // control-char arm already closes on `:repo`). Pinned
        // separately from the NUL arm so a future relaxation that
        // catches one but not the other surfaces here.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia\n".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoControlChar { byte, .. } = err else {
            panic!("expected FonteCaminhoControlChar, got {err:?}");
        };
        assert_eq!(byte, 0x0A);
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_embedded_carriage_return() {
        // The CRLF sibling of the LF arm — Windows-line-ending
        // paste-from-multiline-doc on a `\r\n`-terminated buffer
        // leaves a stray `\r` mid-string after the LF strip. Pinned
        // separately from the LF arm so a future relaxation that
        // only catches LF surfaces here.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia\r".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoControlChar { byte, .. } = err else {
            panic!("expected FonteCaminhoControlChar, got {err:?}");
        };
        assert_eq!(byte, 0x0D);
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_embedded_tab() {
        // The canonical paste-from-aligned-table footgun — a `\t`
        // mid-`:caminho` is invisible in most editors but rides
        // through the lacre's content-address verbatim, so two
        // paste-from-distinct-tables (one editor strips tabs, one
        // preserves them) yield divergent lacres for the byte-
        // identical-looking caixa. Pinned separately from the
        // whitespace-shaped LF/CR arms so a future relaxation that
        // narrows to line-terminator-only surfaces here.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa\tteia".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoControlChar { byte, .. } = err else {
            panic!("expected FonteCaminhoControlChar, got {err:?}");
        };
        assert_eq!(byte, 0x09);
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_embedded_del() {
        // The DEL byte (`0x7F`) closes the upper-end paste-from-
        // binary-blob footgun — the gate's contract is `b < 0x20 ||
        // b == 0x7F`, matching the `is_git_repo_url` /
        // `is_git_ref_name` predicates' control-char arms. Pinned
        // separately from the lower-range arms so a future narrowing
        // to `< 0x20` only surfaces here.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa\x7fteia".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoControlChar { byte, .. } = err else {
            panic!("expected FonteCaminhoControlChar, got {err:?}");
        };
        assert_eq!(byte, 0x7F);
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_high_bit_utf8() {
        // The control-byte arm targets `0x00..=0x1F` + `0x7F` only —
        // high-bit / non-ASCII UTF-8 bytes are not gated. POSIX paths
        // are opaque byte sequences and UTF-8 multi-byte sequences
        // are a legitimate filename shape (the `café-teia/foo` idiom).
        // Pinned so the gate doesn't widen to a full ASCII-only sweep
        // that would break every legitimate-shape UTF-8 path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../café-teia/foo".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_var_fires_before_control_char() {
        // Cascade pin: the var-expansion arm structurally precedes the
        // control-char arm. A value like `"$\n"` probes positive on
        // both arms (`starts_with('$')` and contains LF), but the
        // narrower leading-byte diagnostic (`FonteCaminhoVarExpansion`)
        // wins so the author sees the more self-locating shell-
        // expansion arm first. Mirrors the
        // `fonte_caminho_tilde_fires_before_var_expansion` cascade
        // discipline on the immediate-predecessor arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "$HOME\n".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoVarExpansion { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_leading_space_caminho() {
        // The fail-before-pass-after pin for the leading ASCII space
        // `:caminho` shape: `(:tipo path :caminho " ../caixa-teia")`.
        // Until this gate landed the b94fd83 absolute arm + the a5c248e
        // tilde arm + the f4efe9c var arm + the d624c8d control-byte arm
        // all let `" ../caixa-teia"` through: `Path::is_absolute` returns
        // false on a leading space (the leading byte is `0x20`, not `0x2F`),
        // `starts_with('~')` / `starts_with('$')` return false, and `0x20`
        // is not in the `0x00..=0x1F` plus `0x7F` control-byte set (the
        // four ASCII whitespace bytes `0x09` tab, `0x0A` LF, `0x0D` CR
        // are caught, but the most common whitespace `0x20` space is
        // not). The lacre embedded the value verbatim and the resolver
        // folded it through `Path::join` looking for a literal `./ ../
        // caixa-teia` subdirectory and failing at resolve time with a
        // non-self-locating `No such file or directory` error far from
        // the source caixa.lisp. The new gate moves the check to
        // validate time and names the offending dep + caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: " ../caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoLeadingWhitespace { nome, caminho } = err else {
            panic!("expected FonteCaminhoLeadingWhitespace, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, " ../caixa-teia");
    }

    #[test]
    fn validate_rejects_path_fonte_with_multiple_leading_spaces_caminho() {
        // The aligned-doc paste footgun sweep: more than one leading
        // space (`"   ../caixa-teia"` — the canonical "I selected the
        // aligned column from a four-`:fonte`-entry `:deps` block"
        // paste) routes through the same gate's `starts_with(' ')`
        // byte check. Pinned so the gate doesn't narrow to a
        // single-space prefix.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "   ../caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoLeadingWhitespace { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_mid_path_space_caminho() {
        // The leading-space is the canonical paste-from-aligned-doc
        // footgun — a space mid-path (`"../my dir/caixa-teia"` — the
        // canonical "I have a directory with a space in its name"
        // idiom; ASCII `0x20` is a valid POSIX filename byte) is a
        // legitimate path with no whitespace-leak semantic at the
        // non-leading position. Pinned so the gate doesn't widen to a
        // full no-space-anywhere sweep that would break every
        // legitimate-shape space-in-filename path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../my dir/caixa-teia".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_var_fires_before_leading_whitespace() {
        // Cascade pin: the var-expansion arm structurally precedes the
        // leading-whitespace arm. A value like `"$ "` would probe positive
        // on var (`starts_with('$')`) but the leading-byte arms walk
        // left-to-right so the var arm fires on the leading `$` before
        // the leading-whitespace arm probes. Mirrors the
        // `fonte_caminho_tilde_fires_before_var_expansion` cascade
        // discipline on the immediate-predecessor arms.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "$VAR".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoVarExpansion { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_leading_whitespace_fires_before_control_char() {
        // Cascade pin: the leading-whitespace arm structurally precedes
        // the control-char arm. A value like `" ../foo\n"` probes
        // positive on both (starts with space AND contains LF), but
        // the narrower leading-byte diagnostic
        // (`FonteCaminhoLeadingWhitespace`) wins so the author sees the
        // more self-locating paste-from-aligned-doc arm first. Mirrors
        // the `fonte_caminho_var_fires_before_control_char` cascade
        // discipline on the immediate-predecessor arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: " ../foo\n".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoLeadingWhitespace { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_leading_whitespace_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_var_diagnostic_carries_offending_dep_and_caminho`'s
        // payload assertion on the immediate-predecessor arm): the
        // error's Display surfaces both the offending `:nome` and the
        // offending `:caminho` verbatim, so a `feira lint` run can
        // render the diagnostic without re-parsing and the author can
        // grep their caixa.lisp for `:caminho "<value>"` and fix it in
        // one edit.
        let d = dep_with_fonte(DepSource::Path {
            caminho: " ../caixa-teia".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains(" ../caixa-teia"),
            "diagnostic must quote the offending caminho: {rendered}",
        );
        assert!(
            rendered.contains("space"),
            "diagnostic must name the space footgun: {rendered}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_control_char() {
        // Cascade pin on the sibling leading-byte arm: a leading `/`
        // value with embedded control byte (`"/etc/passwd\n"`) routes
        // through `FonteCaminhoAbsolute` not `FonteCaminhoControlChar`
        // — the host-layout-leak diagnostic is the load-bearing axis,
        // the control byte is the secondary observation. Same precedence
        // logic on every prior leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/passwd\n".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_control_diagnostic_carries_offending_dep_caminho_byte() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_var_diagnostic_carries_offending_dep_and_caminho`'s
        // payload assertion on the immediate-predecessor arm): the
        // error's Display surfaces the offending `:nome`, the
        // offending `:caminho` verbatim, and the offending byte in
        // hex form (`0x09` for tab) so a `feira lint` run can render
        // the diagnostic without re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa\tteia".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("../caixa\tteia"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains("0x09"),
            "diagnostic must name the offending byte in hex: {rendered:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_backslash() {
        // The fail-before-pass-after pin for the canonical Windows-
        // path-separator paste footgun: an author who pastes a path
        // from Windows-Explorer's `Copy as path`, PowerShell's
        // `Get-Location`, or any CMD/Cygwin/MSYS shell prompt
        // produces `..\caixa-teia`-shape values that silently passed
        // every prior arm (`Path::is_absolute("..\\caixa-teia")` is
        // false; `\` is neither a leading-byte sentinel nor a
        // control byte). On POSIX resolvers the value rides through
        // `Path::join` as a literal directory name and fails at
        // resolve time with `No such file or directory`; on Windows
        // resolvers the value resolves to the parent's sibling — two
        // distinct directories for the byte-identical caixa.lisp.
        // The new arm moves the rejection to validate time and names
        // the offending dep + caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoBackslash { nome, caminho } = err else {
            panic!("expected FonteCaminhoBackslash, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "..\\caixa-teia");
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_windows_drive_letter() {
        // The Windows drive-letter paste shape (`C:\work\caixa-teia`
        // out of Explorer / `cd`-and-`pwd`-on-Windows). On POSIX
        // hosts `Path::is_absolute("C:\\work\\caixa-teia")` returns
        // false (POSIX absolute paths start with `/`, drive letters
        // are not a POSIX concept), so the b94fd83 absolute arm
        // doesn't fire; the value contains `\` bytes that this arm
        // now catches with the more self-locating Windows-path-
        // separator diagnostic. Pinned separately from the bare
        // `..\caixa-teia` shape so a future arm that targets only
        // leading-`..\` doesn't regress the drive-letter coverage.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "C:\\work\\caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_trailing_backslash() {
        // The trailing-`\` shape (`..\caixa-teia\` — the canonical
        // PowerShell tab-completion-on-a-directory append). Pinned
        // separately from the embedded-`\` shape so the gate's
        // contract is "any `\` anywhere", not "any `\` not at end".
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia\\".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_legitimate_forward_slash() {
        // The positive-control pin: the gate targets `\` only,
        // never `/`. The canonical relative POSIX path
        // (`../caixa-teia/foo/bar`) must continue to validate cleanly
        // so legitimate nested-directory deps aren't broken. Pinned
        // so the gate doesn't accidentally widen to a "no path
        // separators at all" sweep.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/foo/bar".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_control_char_fires_before_backslash() {
        // Cascade pin: the control-char arm structurally precedes the
        // backslash arm. A value like `"..\caixa\0teia"` probes
        // positive on both (`\` byte + NUL byte), but the control-
        // char diagnostic wins so the author sees the more self-
        // locating POSIX-syscall-rejected-byte diagnostic first
        // (NUL outright breaks `CString::new` at every `std::fs`
        // syscall boundary; the `\` divergence is the cross-OS-
        // separator axis). Mirrors the
        // `fonte_caminho_var_fires_before_control_char` cascade
        // discipline on the immediate-predecessor arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa\0teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoControlChar { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_backslash() {
        // Cascade pin on the load-bearing leading-byte arm: a leading
        // `/` value with embedded `\` (`/etc/passwd\foo`) routes
        // through `FonteCaminhoAbsolute` not `FonteCaminhoBackslash`
        // — the host-layout-leak diagnostic is the load-bearing
        // axis, the `\` byte is the secondary observation. Same
        // precedence logic as every prior leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/passwd\\foo".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_var_fires_before_backslash() {
        // Cascade pin on the var-expansion arm: a leading-`$` value
        // with embedded `\` (`$WORKSPACE\caixa-teia` — the canonical
        // PowerShell-env-var paste-from-CI-manifest footgun) routes
        // through `FonteCaminhoVarExpansion` not `FonteCaminhoBackslash`.
        // The shell-expansion diagnostic is the more self-locating
        // axis since both the leading `$` and the embedded `\`
        // are Windows-shell artifacts but the `$` is the root-cause
        // surface (an author who removes the `$` is likely to leave
        // the `\` too).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "$WORKSPACE\\caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoVarExpansion { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_backslash_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with the prior
        // `fonte_caminho_*_diagnostic_carries_*` payload assertions
        // on every preceding arm): the error's Display surfaces the
        // offending `:nome` and the offending `:caminho` verbatim
        // so a `feira lint` run can render the diagnostic without
        // re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("..\\caixa-teia"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains('\\'),
            "diagnostic must reference the backslash footgun: {rendered:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_trailing_slash() {
        // The fail-before-pass-after pin for the canonical trailing-`/`
        // paste footgun: an author who shell-tab-completes a sibling
        // directory (every interactive shell — bash/zsh/fish/nushell —
        // appends `/` on tab-completing a directory) produces
        // `"../caixa-teia/"`-shape values that silently passed every
        // prior arm (the leading byte is `.`, no control bytes, no
        // backslash). `Path::join` resolves both shapes to the same
        // directory at the resolver, but the lacre embeds the value
        // verbatim and the BLAKE3 closures diverge across two
        // workstations whose authors differ only in tab-completion
        // habits.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoTrailingSlash { nome, caminho } = err else {
            panic!("expected FonteCaminhoTrailingSlash, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "../caixa-teia/");
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_bare_dot_slash() {
        // The `"./"` shape (the canonical "I meant the caixa.lisp's own
        // directory and tab-completed it" footgun). Pinned separately
        // from the canonical `"../caixa-teia/"` shape so the gate's
        // contract is "any trailing `/`", not "trailing `/` after a leaf
        // name".
        let d = dep_with_fonte(DepSource::Path {
            caminho: "./".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoTrailingSlash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_consecutive_trailing_slashes() {
        // The `"foo//"` shape (the canonical "I pasted from a CI manifest
        // that double-templated `${VAR}/` over an already-`/`-suffixed
        // path" footgun). The gate fires on the last byte being `/`
        // regardless of how many `/` precede it; the arm contract is
        // "the value ends with `/`", structurally.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia//".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoTrailingSlash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_dotdot_trailing_slash() {
        // The `"../"` shape (the canonical "I want the parent" tab-
        // completion footgun on a bare `..` path). Pinned separately so
        // the gate doesn't accidentally narrow to "trailing `/` only on
        // multi-segment paths".
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoTrailingSlash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_internal_slashes() {
        // The positive-control pin: the gate targets the trailing byte
        // only, never internal `/` separators. The canonical nested
        // relative POSIX path (`"../caixa-teia/foo/bar"`) must continue
        // to validate cleanly so legitimate deeply-nested deps aren't
        // broken. Pinned so the gate doesn't accidentally widen to a
        // "no `/` separators anywhere" sweep that would defeat the
        // entire path-fonte author surface.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/foo/bar".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_bare_dot() {
        // The positive-control pin on the degenerate single-`.` shape
        // (the canonical "the caixa.lisp's own directory" idiom). The
        // gate fires on the trailing byte being `/`, not on the path
        // being short, so `"."` (one byte, not `/`) must continue to
        // validate cleanly.
        let d = dep_with_fonte(DepSource::Path {
            caminho: ".".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_control_char_fires_before_trailing_slash() {
        // Cascade pin: the control-char arm structurally precedes the
        // trailing-slash arm. A value like `"../foo\n/"` ends in `/`
        // but the embedded LF (`0x0A`) is the load-bearing diagnostic
        // (control bytes are the paste-from-multiline-doc footgun the
        // d624c8d arm already closes). Mirrors the
        // `fonte_caminho_control_char_fires_before_backslash` cascade
        // discipline on the immediate-predecessor arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo\n/".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoControlChar { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_backslash_fires_before_trailing_slash() {
        // Cascade pin on the backslash arm: a value like `"..\foo/"`
        // ends in `/` but the embedded `\` is the load-bearing
        // diagnostic (the cross-host-OS-separator divergence vector
        // the 3a4e1d7 arm closes). Same precedence logic as the prior
        // narrower-diagnostic-first cascade.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia/".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_trailing_slash() {
        // Cascade pin on the load-bearing leading-byte arm: a leading
        // `/` value with a trailing `/` (`"/etc/passwd/"`) routes
        // through `FonteCaminhoAbsolute` not `FonteCaminhoTrailingSlash`
        // — the host-layout-leak diagnostic is the load-bearing axis,
        // the trailing `/` is the secondary observation. Same
        // precedence logic as every prior leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/passwd/".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_trailing_slash_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with the prior
        // `fonte_caminho_*_diagnostic_carries_*` payload assertions on
        // every preceding arm): the error's Display surfaces the
        // offending `:nome` and the offending `:caminho` verbatim so a
        // `feira lint` run can render the diagnostic without re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("../caixa-teia/"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains("trailing"),
            "diagnostic must reference the trailing-slash footgun: {rendered:?}",
        );
    }

    // -- :caminho shell-redirection metacharacter arm -----------------------

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_gt_redirection() {
        // The fail-before-pass-after pin for the canonical output-redirection
        // paste footgun: an author copies a shell pipeline tail
        // (`"../caixa-teia>build.log"` — the canonical "I selected the whole
        // line including the `> build.log` redirect" idiom) and silently
        // passed every prior arm (`Path::is_absolute` false on `..`, no
        // control bytes, no backslash, doesn't end in `/`). The lacre
        // embedded the value verbatim, the resolver folded it through
        // `Path::join` looking for a literal `./../caixa-teia>build.log`
        // subdirectory, and the failure surfaced at resolve time with a
        // non-self-locating `No such file or directory` error. The new arm
        // moves the rejection to validate time and names the offending dep
        // + caminho + byte verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia>build.log".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoShellRedirection {
            nome,
            caminho,
            byte,
        } = err
        else {
            panic!("expected FonteCaminhoShellRedirection, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "../caixa-teia>build.log");
        assert_eq!(byte, b'>');
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_lt_redirection() {
        // The symmetric input-redirection paste shape
        // (`"../caixa-teia<input.lisp"` — the canonical "I copied a
        // `command < input.lisp` line from a tatara-lisp REPL log"
        // idiom). Pinned separately from the `>` shape so the gate's
        // contract is "any `<` or `>` anywhere", not single-byte coverage.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia<input.lisp".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoShellRedirection { byte, .. } = err else {
            panic!("expected FonteCaminhoShellRedirection, got {err:?}");
        };
        assert_eq!(byte, b'<');
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_leading_gt_redirection() {
        // Leading-position `>` shape (`">../caixa-teia"` — the degenerate
        // "I forgot the source side of the redirect" idiom). Pinned
        // separately from the embedded-byte shapes so the gate covers
        // every position, not only mid-path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: ">../caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteCaminhoShellRedirection { byte: b'>', .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_double_gt_append() {
        // The bash append-redirection shape (`"../caixa-teia>>build.log"` —
        // the canonical "I copied a `>>` append redirect" idiom). The arm
        // fires on the first `>` encountered; pinned so a future arm that
        // tries to distinguish `>` from `>>` doesn't break the broader
        // contract.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia>>build.log".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteCaminhoShellRedirection { byte: b'>', .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_no_redirection() {
        // The positive-control pin: the gate targets only `<` / `>`,
        // never adjacent printable ASCII or POSIX-valid bytes. The
        // canonical relative POSIX path (`"../caixa-teia"`) and a
        // nested deeply-pathed variant (`"../caixa-teia/foo/bar"`) must
        // continue to validate cleanly so the gate doesn't widen to a
        // "no printable punctuation anywhere" sweep that would defeat
        // the entire path-fonte author surface.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/foo/bar".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_backslash_fires_before_shell_redirection() {
        // Cascade pin on the immediate-predecessor arm: a value carrying
        // both `\` and `<` / `>` (`"..\caixa-teia>build.log"` — the
        // canonical "I pasted a Windows-shell command with output
        // redirect" footgun) routes through `FonteCaminhoBackslash` not
        // `FonteCaminhoShellRedirection`. The cross-host-OS-separator
        // divergence is the load-bearing axis (an author who removes
        // the `\` is the root-cause edit; the `>` falls away in the
        // same edit since it's downstream of the Windows-shell
        // convention).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia>build.log".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_control_char_fires_before_shell_redirection() {
        // Cascade pin on the embedded-control-byte arm: a value carrying
        // both a control byte and `<` / `>` (`"../foo\n>bar"` — the
        // canonical paste-from-multiline-doc footgun where a newline
        // landed mid-caminho) routes through `FonteCaminhoControlChar`
        // not `FonteCaminhoShellRedirection`. The POSIX-syscall-
        // rejected-byte / NUL-`CString::new`-fail diagnostic is the
        // load-bearing axis on every value that probes positive for
        // both — mirrors the cascade discipline on every prior arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo\n>bar".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoControlChar { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_shell_redirection() {
        // Cascade pin on the load-bearing leading-byte arm: a leading
        // `/` value with embedded `<` / `>` (`"/etc/passwd>out"`)
        // routes through `FonteCaminhoAbsolute` not
        // `FonteCaminhoShellRedirection` — the host-layout-leak
        // diagnostic is the load-bearing axis, the `>` byte is the
        // secondary observation. Same precedence logic as every prior
        // leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/passwd>out".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_redirection_fires_before_trailing_slash() {
        // Cascade pin on the immediate-successor arm: a value carrying
        // both `<` / `>` and a trailing `/` (`"../foo></"` — the
        // canonical "I tab-completed a path that already had a
        // redirect" footgun) routes through `FonteCaminhoShellRedirection`
        // not `FonteCaminhoTrailingSlash`. The embedded shell-metachar is
        // the more semantic-locating axis (an author who removes the
        // `<` / `>` typically also drops the trailing separator since
        // both are paste-from-shell artifacts).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo></".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteCaminhoShellRedirection { byte: b'>', .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_redirection_diagnostic_carries_offending_dep_caminho_byte() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_control_diagnostic_carries_offending_dep_caminho_byte`'s
        // payload assertion on the closest peer arm that also carries a
        // `byte` field): the error's Display surfaces the offending
        // `:nome`, the offending `:caminho` verbatim, and the offending
        // byte in hex (`0x3c` for `<`, `0x3e` for `>`) so a `feira lint`
        // run can render the diagnostic without re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia>build.log".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("../caixa-teia>build.log"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains("0x3e"),
            "diagnostic must name the offending byte in hex: {rendered:?}",
        );
        assert!(
            rendered.contains("redirection"),
            "diagnostic must name the shell-redirection footgun: {rendered:?}",
        );
    }

    // -- :caminho shell-pipe metacharacter arm ----------------------------

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_pipe() {
        // The fail-before-pass-after pin for the canonical shell-pipe
        // paste footgun: an author copies a shell-history line
        // (`"../caixa-teia | grep foo"` — the canonical "I selected
        // the whole `ls dir | grep` line out of zsh history") and
        // silently passed every prior arm (`Path::is_absolute` false
        // on `..`, no control bytes, no backslash, no `<` / `>`,
        // doesn't end in `/`). The lacre embedded the value verbatim,
        // the resolver folded it through `Path::join` looking for a
        // literal `./../caixa-teia | grep foo` subdirectory, and the
        // failure surfaced at resolve time with a non-self-locating
        // `No such file or directory` error. The new arm moves the
        // rejection to validate time and names the offending dep +
        // caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia | grep foo".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoShellPipe { nome, caminho } = err else {
            panic!("expected FonteCaminhoShellPipe, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "../caixa-teia | grep foo");
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_leading_pipe() {
        // Leading-position `|` shape (`"|../caixa-teia"` — the
        // degenerate "I forgot the source side of the pipe" idiom).
        // Pinned separately from the embedded-byte shape so the gate
        // covers every position, not only mid-path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "|../caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellPipe { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_double_pipe_or() {
        // The bash short-circuit-OR shape (`"../caixa-teia||fallback"`
        // — the canonical "I copied a `cmd-a || cmd-b` fallback line"
        // idiom). The arm fires on the first `|` encountered; pinned
        // so a future arm that tries to distinguish `|` from `||`
        // doesn't break the broader contract.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia||fallback".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellPipe { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_no_pipe() {
        // The positive-control pin: the gate targets only `|`, never
        // adjacent printable ASCII or POSIX-valid bytes. The canonical
        // relative POSIX path (`"../caixa-teia"`) and a nested deeply-
        // pathed variant with adjacent printable punctuation
        // (`"../caixa-teia/sub-dir.v2"`) must continue to validate
        // cleanly so the gate doesn't widen to a "no printable
        // punctuation anywhere" sweep that would defeat the entire
        // path-fonte author surface.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/sub-dir.v2".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_shell_redirection_fires_before_shell_pipe() {
        // Cascade pin on the immediate-predecessor arm: a value carrying
        // both `<` / `>` and `|` (`"../caixa-teia<input|tee"` — the
        // canonical "I pasted a `cmd < input | tee` pipeline tail"
        // footgun) routes through `FonteCaminhoShellRedirection` not
        // `FonteCaminhoShellPipe`. The input/output redirection
        // metachar carries the more self-locating `byte: u8` payload
        // (it names which of `<` or `>` triggered), so the prior arm
        // wins on every probe-as-both value — same cascade discipline
        // every prior `:caminho` arm establishes.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia<input|tee".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteCaminhoShellRedirection { byte: b'<', .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_backslash_fires_before_shell_pipe() {
        // Cascade pin on the upstream backslash arm: a value carrying
        // both `\` and `|` (`"..\caixa-teia|tee"` — the canonical
        // "I pasted a Windows-shell command with pipe to tee"
        // footgun) routes through `FonteCaminhoBackslash` not
        // `FonteCaminhoShellPipe`. The cross-host-OS-separator
        // divergence is the load-bearing axis on every probe-as-both
        // value (an author who removes the `\` is the root-cause edit;
        // the `|` falls away in the same edit since it's downstream of
        // the Windows-shell convention).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia|tee".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_control_char_fires_before_shell_pipe() {
        // Cascade pin on the embedded-control-byte arm: a value
        // carrying both a control byte and `|` (`"../foo\n|bar"` —
        // the canonical paste-from-multiline-doc footgun where a
        // newline landed mid-caminho) routes through
        // `FonteCaminhoControlChar` not `FonteCaminhoShellPipe`. The
        // POSIX-syscall-rejected-byte / NUL-`CString::new`-fail
        // diagnostic is the load-bearing axis on every value that
        // probes positive for both — mirrors the cascade discipline
        // on every prior arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo\n|bar".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoControlChar { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_shell_pipe() {
        // Cascade pin on the load-bearing leading-byte arm: a leading
        // `/` value with embedded `|` (`"/etc/passwd|tee"`) routes
        // through `FonteCaminhoAbsolute` not `FonteCaminhoShellPipe`
        // — the host-layout-leak diagnostic is the load-bearing axis,
        // the `|` byte is the secondary observation. Same precedence
        // logic as every prior leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/passwd|tee".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_pipe_fires_before_trailing_slash() {
        // Cascade pin on the immediate-successor arm: a value carrying
        // both `|` and a trailing `/` (`"../foo|tee/"` — the canonical
        // "I tab-completed a path that already had a pipeline tail"
        // footgun) routes through `FonteCaminhoShellPipe` not
        // `FonteCaminhoTrailingSlash`. The embedded shell-metachar is
        // the more semantic-locating axis (an author who removes the
        // `|` typically also drops the trailing separator since both
        // are paste-from-shell artifacts).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo|tee/".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellPipe { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_pipe_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_backslash_diagnostic_carries_offending_dep_and_caminho`
        // on the closest single-byte peer arm): the error's Display
        // surfaces the offending `:nome` and the offending `:caminho`
        // verbatim, and names the shell-pipe footgun explicitly so a
        // `feira lint` run can render the diagnostic without
        // re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia | grep foo".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("../caixa-teia | grep foo"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains('|'),
            "diagnostic must reference the pipe footgun: {rendered:?}",
        );
        assert!(
            rendered.contains("pipe"),
            "diagnostic must name the shell-pipe footgun: {rendered:?}",
        );
    }

    // -- :caminho shell-command-separator metacharacter arm ---------------

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_semicolon() {
        // The fail-before-pass-after pin for the canonical shell-command-
        // separator paste footgun: an author copies a shell one-liner
        // (`"../caixa-teia; rm -rf build"` — the canonical "I selected the
        // whole `cd path; do-thing` chain out of a shell-history block")
        // and silently passed every prior arm (`Path::is_absolute` false
        // on `..`, no control bytes, no backslash, no `<` / `>`, no `|`,
        // doesn't end in `/`). The lacre embedded the value verbatim, the
        // resolver folded it through `Path::join` looking for a literal
        // `./../caixa-teia; rm -rf build` subdirectory, and the failure
        // surfaced at resolve time with a non-self-locating `No such file
        // or directory` error. The new arm moves the rejection to validate
        // time and names the offending dep + caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia; rm -rf build".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoShellSemicolon { nome, caminho } = err else {
            panic!("expected FonteCaminhoShellSemicolon, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "../caixa-teia; rm -rf build");
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_leading_semicolon() {
        // Leading-position `;` shape (`";../caixa-teia"` — the degenerate
        // "I forgot the prior command side of the separator" idiom).
        // Pinned separately from the embedded-byte shape so the gate
        // covers every position, not only mid-path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: ";../caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellSemicolon { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_double_semicolon() {
        // The POSIX `case` arm `;;` terminator shape
        // (`"../caixa-teia;;next"` — the canonical "I copied a `case`
        // arm tail" idiom). The arm fires on the first `;` encountered;
        // pinned so a future arm that tries to distinguish `;` from `;;`
        // doesn't break the broader contract.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia;;next".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellSemicolon { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_no_semicolon() {
        // The positive-control pin: the gate targets only `;`, never
        // adjacent printable ASCII or POSIX-valid bytes. The canonical
        // relative POSIX path (`"../caixa-teia"`) and a nested deeply-
        // pathed variant with adjacent printable punctuation
        // (`"../caixa-teia/sub-dir.v2"`) must continue to validate
        // cleanly so the gate doesn't widen to a "no printable
        // punctuation anywhere" sweep that would defeat the entire
        // path-fonte author surface.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/sub-dir.v2".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_shell_pipe_fires_before_shell_semicolon() {
        // Cascade pin on the immediate-predecessor arm: a value carrying
        // both `|` and `;` (`"../caixa-teia | tee; rm"` — the canonical
        // "I pasted a `cmd | tee; cleanup` chain" footgun) routes through
        // `FonteCaminhoShellPipe` not `FonteCaminhoShellSemicolon`. The
        // pipeline-tail paste is the load-bearing root-cause edit on
        // every probe-as-both value (an author who removes the `|`
        // typically also drops the trailing `; cleanup` since both are
        // the same paste-from-shell-history artifact) — same cascade
        // discipline every prior `:caminho` arm establishes.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia | tee; rm".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellPipe { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_redirection_fires_before_shell_semicolon() {
        // Cascade pin on the upstream shell-redirection arm: a value
        // carrying both `<` / `>` and `;` (`"../caixa-teia>log; rm"` —
        // the canonical "I pasted a `cmd > log; cleanup` chain"
        // footgun) routes through `FonteCaminhoShellRedirection` not
        // `FonteCaminhoShellSemicolon`. The input/output redirection
        // metachar carries the more self-locating `byte: u8` payload
        // (it names which of `<` or `>` triggered), so the prior arm
        // wins on every probe-as-both value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia>log; rm".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteCaminhoShellRedirection { byte: b'>', .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_backslash_fires_before_shell_semicolon() {
        // Cascade pin on the upstream backslash arm: a value carrying
        // both `\` and `;` (`"..\caixa-teia;rm"` — the canonical "I
        // pasted a Windows-shell `cd ..\path; cleanup` chain") routes
        // through `FonteCaminhoBackslash` not `FonteCaminhoShellSemicolon`.
        // The cross-host-OS-separator divergence is the load-bearing axis
        // on every probe-as-both value (an author who removes the `\` is
        // the root-cause edit; the `;` falls away in the same edit since
        // it's downstream of the Windows-shell convention).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia;rm".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_control_char_fires_before_shell_semicolon() {
        // Cascade pin on the embedded-control-byte arm: a value carrying
        // both a control byte and `;` (`"../foo\n;bar"` — the canonical
        // paste-from-multiline-doc footgun where a newline landed mid-
        // caminho) routes through `FonteCaminhoControlChar` not
        // `FonteCaminhoShellSemicolon`. The POSIX-syscall-rejected-byte
        // / NUL-`CString::new`-fail diagnostic is the load-bearing axis
        // on every value that probes positive for both — mirrors the
        // cascade discipline on every prior arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo\n;bar".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoControlChar { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_shell_semicolon() {
        // Cascade pin on the load-bearing leading-byte arm: a leading
        // `/` value with embedded `;` (`"/etc/passwd;rm"`) routes
        // through `FonteCaminhoAbsolute` not `FonteCaminhoShellSemicolon`
        // — the host-layout-leak diagnostic is the load-bearing axis,
        // the `;` byte is the secondary observation. Same precedence
        // logic as every prior leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/passwd;rm".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_semicolon_fires_before_trailing_slash() {
        // Cascade pin on the immediate-successor arm: a value carrying
        // both `;` and a trailing `/` (`"../foo;rm/"` — the canonical
        // "I tab-completed a path that already had a `; cleanup` tail"
        // footgun) routes through `FonteCaminhoShellSemicolon` not
        // `FonteCaminhoTrailingSlash`. The embedded shell-metachar is
        // the more semantic-locating axis (an author who removes the
        // `;` typically also drops the trailing separator since both
        // are paste-from-shell artifacts).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo;rm/".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellSemicolon { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_semicolon_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_shell_pipe_diagnostic_carries_offending_dep_and_caminho`
        // on the closest single-byte peer arm): the error's Display
        // surfaces the offending `:nome` and the offending `:caminho`
        // verbatim, and names the shell-command-separator footgun
        // explicitly so a `feira lint` run can render the diagnostic
        // without re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia; rm -rf build".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("../caixa-teia; rm -rf build"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains(';'),
            "diagnostic must reference the semicolon footgun: {rendered:?}",
        );
        assert!(
            rendered.contains("command-separator"),
            "diagnostic must name the shell-command-separator footgun: {rendered:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_ampersand() {
        // The fail-before-pass-after pin for the canonical shell-
        // background-task paste footgun: an author copies a shell one-
        // liner (`"../caixa-teia & sleep 1"` — the canonical "I selected
        // the whole `cd path & sleep 1` background-launch out of a
        // shell-history block") and silently passed every prior arm
        // (`Path::is_absolute` false on `..`, no control bytes, no
        // backslash, no `<` / `>`, no `|`, no `;`, doesn't end in `/`).
        // The lacre embedded the value verbatim, the resolver folded it
        // through `Path::join` looking for a literal `./../caixa-teia &
        // sleep 1` subdirectory, and the failure surfaced at resolve
        // time with a non-self-locating `No such file or directory`
        // error. The new arm moves the rejection to validate time and
        // names the offending dep + caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia & sleep 1".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoShellBackground { nome, caminho } = err else {
            panic!("expected FonteCaminhoShellBackground, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "../caixa-teia & sleep 1");
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_leading_ampersand() {
        // Leading-position `&` shape (`"&../caixa-teia"` — the
        // degenerate "I forgot the prior command side of the
        // background terminator" idiom). Pinned separately from the
        // embedded-byte shape so the gate covers every position, not
        // only mid-path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "&../caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellBackground { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_double_ampersand() {
        // The logical-AND `&&` shape (`"../caixa-teia && make"` — the
        // canonical "I copied a `cd path && make` build chain" idiom
        // every Makefile / shell-script wraps). The arm fires on the
        // first `&` encountered; pinned so a future arm that tries to
        // distinguish `&` from `&&` doesn't break the broader contract.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia && make".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellBackground { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_no_ampersand() {
        // The positive-control pin: the gate targets only `&`, never
        // adjacent printable ASCII or POSIX-valid bytes. The canonical
        // relative POSIX path (`"../caixa-teia"`) and a nested deeply-
        // pathed variant with adjacent printable punctuation
        // (`"../caixa-teia/sub-dir.v2"`) must continue to validate
        // cleanly so the gate doesn't widen to a "no printable
        // punctuation anywhere" sweep that would defeat the entire
        // path-fonte author surface.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/sub-dir.v2".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_shell_semicolon_fires_before_shell_background() {
        // Cascade pin on the immediate-predecessor arm: a value carrying
        // both `;` and `&` (`"../caixa-teia; rm & sleep"` — the
        // canonical "I pasted a `cmd; cleanup & sleep` chain" footgun)
        // routes through `FonteCaminhoShellSemicolon` not
        // `FonteCaminhoShellBackground`. The sequential-command-
        // separator paste is the more common shell-history paste idiom
        // on every probe-as-both value (an author who removes the `;`
        // typically also drops the trailing `& sleep` since both are
        // paste-from-shell-history artifacts) — same cascade discipline
        // every prior `:caminho` arm establishes.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia; rm & sleep".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellSemicolon { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_pipe_fires_before_shell_background() {
        // Cascade pin on the upstream shell-pipe arm: a value carrying
        // both `|` and `&` (`"../caixa-teia | tee & sleep"` — the
        // canonical "I pasted a `cmd | tee & sleep` background-pipeline
        // chain" footgun) routes through `FonteCaminhoShellPipe` not
        // `FonteCaminhoShellBackground`. The pipeline-tail paste is the
        // load-bearing root-cause edit on every probe-as-both value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia | tee & sleep".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellPipe { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_redirection_fires_before_shell_background() {
        // Cascade pin on the upstream shell-redirection arm: a value
        // carrying both `>` and `&` (`"../caixa-teia>log & sleep"` —
        // the canonical "I pasted a `cmd > log & sleep` background-
        // redirect chain" footgun) routes through
        // `FonteCaminhoShellRedirection` not
        // `FonteCaminhoShellBackground`. The input/output redirection
        // metachar carries the more self-locating `byte: u8` payload
        // (it names which of `<` or `>` triggered), so the prior arm
        // wins on every probe-as-both value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia>log & sleep".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteCaminhoShellRedirection { byte: b'>', .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_backslash_fires_before_shell_background() {
        // Cascade pin on the upstream backslash arm: a value carrying
        // both `\` and `&` (`"..\caixa-teia & sleep"` — the canonical
        // "I pasted a Windows-shell `cd ..\path & sleep` background-
        // launch chain") routes through `FonteCaminhoBackslash` not
        // `FonteCaminhoShellBackground`. The cross-host-OS-separator
        // divergence is the load-bearing axis on every probe-as-both
        // value (an author who removes the `\` is the root-cause edit;
        // the `&` falls away in the same edit since it's downstream of
        // the Windows-shell convention).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia & sleep".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_control_char_fires_before_shell_background() {
        // Cascade pin on the embedded-control-byte arm: a value
        // carrying both a control byte and `&` (`"../foo\n&sleep"` —
        // the canonical paste-from-multiline-doc footgun where a
        // newline landed mid-caminho) routes through
        // `FonteCaminhoControlChar` not `FonteCaminhoShellBackground`.
        // The POSIX-syscall-rejected-byte / NUL-`CString::new`-fail
        // diagnostic is the load-bearing axis on every value that
        // probes positive for both — mirrors the cascade discipline on
        // every prior arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo\n&sleep".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoControlChar { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_shell_background() {
        // Cascade pin on the load-bearing leading-byte arm: a leading
        // `/` value with embedded `&` (`"/etc/passwd & sleep"`) routes
        // through `FonteCaminhoAbsolute` not
        // `FonteCaminhoShellBackground` — the host-layout-leak
        // diagnostic is the load-bearing axis, the `&` byte is the
        // secondary observation. Same precedence logic as every prior
        // leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/passwd & sleep".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_background_fires_before_trailing_slash() {
        // Cascade pin on the immediate-successor arm: a value carrying
        // both `&` and a trailing `/` (`"../foo&sleep/"` — the
        // canonical "I tab-completed a path that already had a `&
        // sleep` background-launch tail" footgun) routes through
        // `FonteCaminhoShellBackground` not `FonteCaminhoTrailingSlash`.
        // The embedded shell-metachar is the more semantic-locating
        // axis (an author who removes the `&` typically also drops
        // the trailing separator since both are paste-from-shell
        // artifacts).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo&sleep/".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellBackground { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_background_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_shell_semicolon_diagnostic_carries_offending_dep_and_caminho`
        // on the closest single-byte peer arm): the error's Display
        // surfaces the offending `:nome` and the offending `:caminho`
        // verbatim, and names the shell-background / logical-AND
        // footgun explicitly so a `feira lint` run can render the
        // diagnostic without re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia & sleep 1".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("../caixa-teia & sleep 1"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains('&'),
            "diagnostic must reference the ampersand footgun: {rendered:?}",
        );
        assert!(
            rendered.contains("background") || rendered.contains("list-AND"),
            "diagnostic must name the shell-background / logical-AND footgun: {rendered:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_backtick() {
        // The fail-before-pass-after pin for the canonical shell-
        // command-substitution paste footgun: an author copies a
        // POSIX legacy backticked one-liner (`"../caixa-teia/`whoami`"`
        // — the canonical "I pasted a path that included a `pwd`
        // / `whoami` / `date` legacy command-substitution expansion
        // out of a shell-history block") and silently passed every
        // prior arm (`Path::is_absolute` false on `..`, no control
        // bytes, no `\`, no `<` / `>`, no `|`, no `;`, no `&`, doesn't
        // end in `/`). The lacre embedded the value verbatim, the
        // resolver folded it through `Path::join` looking for a
        // literal `./../caixa-teia/`whoami`` subdirectory, and the
        // failure surfaced at resolve time with a non-self-locating
        // `No such file or directory` error. The new arm moves the
        // rejection to validate time and names the offending dep +
        // caminho verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/`whoami`".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoShellCommandSubstitution { nome, caminho } = err else {
            panic!("expected FonteCaminhoShellCommandSubstitution, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "../caixa-teia/`whoami`");
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_leading_backtick() {
        // Leading-position backtick shape (``"`pwd`/caixa-teia"`` —
        // the canonical `<backtick>pwd<backtick>/path` working-
        // directory expansion shape every shell-side path-composition
        // idiom carries). Pinned separately from the embedded-byte
        // shape so the gate covers every position, not only mid-path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "`pwd`/caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellCommandSubstitution { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_trailing_backtick() {
        // Trailing-position backtick shape (`"../caixa-teia`"` — the
        // degenerate "I selected an unbalanced backtick out of a
        // shell-history block" idiom that probes for the cascade's
        // last-byte handling). The trailing-`/` arm fires only on
        // last-byte `/`; an unbalanced trailing backtick must route
        // through this arm regardless of position.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellCommandSubstitution { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_balanced_backtick_pair() {
        // The canonical balanced-pair shape (``"../<backtick>cat
        // /etc/passwd<backtick>"`` — the canonical CWE-78 shell-
        // command-injection paste idiom every shell-side hardening
        // guide enumerates first). The arm fires on the first
        // backtick encountered; pinned so a future arm that tries to
        // distinguish the opening from the closing byte doesn't break
        // the broader contract.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../`cat /etc/passwd`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellCommandSubstitution { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_no_backtick() {
        // The positive-control pin: the gate targets only the
        // backtick byte, never adjacent printable ASCII or POSIX-
        // valid bytes. The canonical relative POSIX path
        // (`"../caixa-teia"`) and a nested deeply-pathed variant with
        // adjacent printable punctuation
        // (`"../caixa-teia/sub-dir.v2"`) must continue to validate
        // cleanly so the gate doesn't widen to a "no printable
        // punctuation anywhere" sweep that would defeat the entire
        // path-fonte author surface.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/sub-dir.v2".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_shell_background_fires_before_shell_command_substitution() {
        // Cascade pin on the immediate-predecessor arm: a value
        // carrying both `&` and a backtick (``"../caixa-teia &
        // <backtick>sleep 1<backtick>"`` — the canonical "I pasted a
        // `cmd & <backtick>sleep N<backtick>` background-launch +
        // command-substitution chain" footgun) routes through
        // `FonteCaminhoShellBackground` not
        // `FonteCaminhoShellCommandSubstitution`. The background-
        // launch tail is the more common shell-history paste idiom
        // on every probe-as-both value — same cascade discipline
        // every prior `:caminho` arm establishes.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia & `sleep 1`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellBackground { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_semicolon_fires_before_shell_command_substitution() {
        // Cascade pin on the upstream shell-semicolon arm: a value
        // carrying both `;` and a backtick (``"../caixa-teia;
        // <backtick>whoami<backtick>"`` — the canonical "I pasted a
        // `cmd; <backtick>follow-up<backtick>` sequential-chain
        // footgun) routes through `FonteCaminhoShellSemicolon` not
        // `FonteCaminhoShellCommandSubstitution`. The sequential-
        // command-separator paste is the load-bearing root-cause
        // edit on every probe-as-both value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia; `whoami`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellSemicolon { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_pipe_fires_before_shell_command_substitution() {
        // Cascade pin on the upstream shell-pipe arm: a value
        // carrying both `|` and a backtick (``"../caixa-teia |
        // <backtick>tee log<backtick>"`` — the canonical pipeline-to-
        // command-substitution paste idiom) routes through
        // `FonteCaminhoShellPipe` not
        // `FonteCaminhoShellCommandSubstitution`. The pipeline-tail
        // paste is the load-bearing root-cause edit on every
        // probe-as-both value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia | `tee log`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellPipe { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_redirection_fires_before_shell_command_substitution() {
        // Cascade pin on the upstream shell-redirection arm: a value
        // carrying both `>` and a backtick (``"../caixa-teia>log
        // <backtick>date<backtick>"`` — the canonical "I pasted a
        // `cmd > log <backtick>date<backtick>` redirect-plus-
        // substitution chain" footgun) routes through
        // `FonteCaminhoShellRedirection` not
        // `FonteCaminhoShellCommandSubstitution`. The input/output
        // redirection metachar carries the more self-locating `byte`
        // payload (it names which of `<` or `>` triggered), so the
        // prior arm wins on every probe-as-both value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia>log `date`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteCaminhoShellRedirection { byte: b'>', .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_backslash_fires_before_shell_command_substitution() {
        // Cascade pin on the upstream backslash arm: a value
        // carrying both `\` and a backtick (``"..\caixa-teia
        // <backtick>whoami<backtick>"`` — the canonical "I pasted a
        // Windows-shell `cd ..\path <backtick>whoami<backtick>`
        // chain") routes through `FonteCaminhoBackslash` not
        // `FonteCaminhoShellCommandSubstitution`. The cross-host-OS-
        // separator divergence is the load-bearing axis on every
        // probe-as-both value (an author who removes the `\` is the
        // root-cause edit; the backtick falls away in the same edit
        // since it's downstream of the Windows-shell convention).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia `whoami`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_control_char_fires_before_shell_command_substitution() {
        // Cascade pin on the embedded-control-byte arm: a value
        // carrying both a control byte and a backtick (`"../foo\n
        // `whoami`"` — the canonical paste-from-multiline-doc
        // footgun where a newline landed mid-caminho between two
        // paste fragments) routes through `FonteCaminhoControlChar`
        // not `FonteCaminhoShellCommandSubstitution`. The POSIX-
        // syscall-rejected-byte / NUL-`CString::new`-fail diagnostic
        // is the load-bearing axis on every value that probes
        // positive for both — mirrors the cascade discipline on
        // every prior arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo\n`whoami`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoControlChar { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_shell_command_substitution() {
        // Cascade pin on the load-bearing leading-byte arm: a
        // leading `/` value with embedded backtick (``"/etc/passwd
        // <backtick>whoami<backtick>"``) routes through
        // `FonteCaminhoAbsolute` not
        // `FonteCaminhoShellCommandSubstitution` — the host-layout-
        // leak diagnostic is the load-bearing axis, the backtick
        // byte is the secondary observation. Same precedence logic
        // as every prior leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/passwd `whoami`".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_command_substitution_fires_before_trailing_slash() {
        // Cascade pin on the immediate-successor arm: a value
        // carrying both a backtick and a trailing `/`
        // (``"../`whoami`/"`` — the canonical "I tab-completed a
        // path that already had a backticked `whoami` substitution
        // tail" footgun) routes through
        // `FonteCaminhoShellCommandSubstitution` not
        // `FonteCaminhoTrailingSlash`. The embedded shell-metachar
        // is the more semantic-locating axis (an author who removes
        // the backtick typically also drops the trailing separator
        // since both are paste-from-shell artifacts).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../`whoami`/".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellCommandSubstitution { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_command_substitution_diagnostic_carries_offending_dep_and_caminho() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_shell_background_diagnostic_carries_offending_dep_and_caminho`
        // on the closest single-byte peer arm): the error's Display
        // surfaces the offending `:nome` and the offending `:caminho`
        // verbatim, and names the shell-command-substitution footgun
        // explicitly so a `feira lint` run can render the diagnostic
        // without re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/`whoami`".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("../caixa-teia/`whoami`"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains('`'),
            "diagnostic must reference the backtick footgun: {rendered:?}",
        );
        assert!(
            rendered.contains("command-substitution"),
            "diagnostic must name the shell-command-substitution footgun: {rendered:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_star_glob() {
        // The fail-before-pass-after pin for the canonical pathname-
        // expansion paste footgun: an author copies an `ls
        // ../caixa-teia/*` shell-listing tail into the `:caminho`
        // slot and silently passes every prior arm
        // (`Path::is_absolute` false on `..`, no control bytes, no
        // `\`, no `<` / `>`, no `|`, no `;`, no `&`, no backtick,
        // doesn't end in `/`). The lacre embedded the value
        // verbatim, the resolver folded it through `Path::join`
        // looking for a literal `./../caixa-teia/*` subdirectory,
        // and the failure surfaced at resolve time with a non-self-
        // locating `No such file or directory` error. The new arm
        // moves the rejection to validate time and names the
        // offending dep + caminho + byte verbatim.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/*".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoShellGlob {
            nome,
            caminho,
            byte,
        } = err
        else {
            panic!("expected FonteCaminhoShellGlob, got {err:?}");
        };
        assert_eq!(nome, "caixa-teia");
        assert_eq!(caminho, "../caixa-teia/*");
        assert_eq!(byte, b'*');
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_question_glob() {
        // The symmetric single-char-wildcard paste shape
        // (`"../foo?"` — the canonical "I copied a `rm foo?` line
        // out of shell history" idiom). Pinned separately from the
        // `*` shape so the gate's contract is "any `*` or `?`
        // anywhere", not single-byte coverage.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo?".into(),
        });
        let err = d.validate().unwrap_err();
        let DepError::FonteCaminhoShellGlob { byte, .. } = err else {
            panic!("expected FonteCaminhoShellGlob, got {err:?}");
        };
        assert_eq!(byte, b'?');
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_leading_star_glob() {
        // Leading-position `*` shape (`"*/caixa-teia"` — the
        // degenerate "I selected only the wildcard prefix out of a
        // shell-glob expression" idiom). Pinned separately from the
        // embedded-byte shapes so the gate covers every position,
        // not only mid-path.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "*/caixa-teia".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellGlob { byte: b'*', .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_double_star_recursive_glob() {
        // The bash/zsh `globstar` recursive-glob shape
        // (`"../caixa-teia/**/foo"` — the canonical "I copied a
        // `find ../caixa-teia/**/foo` recursive expansion" idiom).
        // The arm fires on the first `*` encountered; pinned so a
        // future arm that tries to distinguish single `*` from
        // double `**` doesn't break the broader contract.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/**/foo".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellGlob { byte: b'*', .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_rejects_path_fonte_with_caminho_carrying_dotted_extension_glob() {
        // The canonical extension-glob shape (`"../caixa-teia/*.lisp"`
        // — the "I selected `*.lisp` to mean every Lisp source file
        // in the dep root" footgun the prior arms structurally
        // cannot catch since `.` is a POSIX-valid path-component
        // byte). Pinned so the gate's contract covers the most
        // idiomatic glob-paste shape every author meets first.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/*.lisp".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellGlob { byte: b'*', .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_accepts_path_fonte_with_caminho_carrying_no_glob() {
        // The positive-control pin: the gate targets only `*` /
        // `?`, never adjacent printable ASCII or POSIX-valid bytes.
        // The canonical relative POSIX path (`"../caixa-teia"`) and
        // a nested deeply-pathed variant with adjacent printable
        // punctuation (`"../caixa-teia/sub-dir.v2"`) must continue
        // to validate cleanly so the gate doesn't widen to a "no
        // printable punctuation anywhere" sweep that would defeat
        // the entire path-fonte author surface.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/sub-dir.v2".into(),
        });
        d.validate().unwrap();
    }

    #[test]
    fn fonte_caminho_shell_command_substitution_fires_before_shell_glob() {
        // Cascade pin on the immediate-predecessor arm: a value
        // carrying both a backtick and `*` (``"../`whoami`/*"`` —
        // the canonical "I pasted a `cd <backtick>whoami<backtick>/*`
        // command-substitution + glob chain") routes through
        // `FonteCaminhoShellCommandSubstitution` not
        // `FonteCaminhoShellGlob`. The CWE-78 shell-command-
        // injection vector is the load-bearing root-cause edit on
        // every probe-as-both value — same cascade discipline every
        // prior `:caminho` arm establishes.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../`whoami`/*".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellCommandSubstitution { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_background_fires_before_shell_glob() {
        // Cascade pin on the upstream shell-background arm: a value
        // carrying both `&` and `*` (`"../caixa-teia & ls /*"` — the
        // canonical "I pasted a `cmd & ls /*` background + glob
        // chain" footgun) routes through `FonteCaminhoShellBackground`
        // not `FonteCaminhoShellGlob`. The background-launch tail is
        // the load-bearing root-cause edit on every probe-as-both
        // value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia & ls /*".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellBackground { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_semicolon_fires_before_shell_glob() {
        // Cascade pin on the upstream shell-semicolon arm: a value
        // carrying both `;` and `*` (`"../caixa-teia; rm *"` — the
        // canonical sequential-cleanup + glob paste idiom) routes
        // through `FonteCaminhoShellSemicolon` not
        // `FonteCaminhoShellGlob`. The sequential-command-separator
        // paste is the load-bearing root-cause edit on every
        // probe-as-both value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia; rm *".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellSemicolon { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_pipe_fires_before_shell_glob() {
        // Cascade pin on the upstream shell-pipe arm: a value
        // carrying both `|` and `*` (`"../caixa-teia | ls *"` — the
        // canonical pipeline-to-glob paste idiom) routes through
        // `FonteCaminhoShellPipe` not `FonteCaminhoShellGlob`. The
        // pipeline-tail paste is the load-bearing root-cause edit
        // on every probe-as-both value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia | ls *".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellPipe { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_redirection_fires_before_shell_glob() {
        // Cascade pin on the upstream shell-redirection arm: a value
        // carrying both `>` and `*` (`"../caixa-teia>log *"` — the
        // canonical "I pasted a `cmd > log *` redirect-plus-glob
        // chain" footgun) routes through
        // `FonteCaminhoShellRedirection` not `FonteCaminhoShellGlob`.
        // The input/output redirection metachar carries the more
        // self-locating `byte` payload (it names which of `<` or `>`
        // triggered), so the prior arm wins on every probe-as-both
        // value.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia>log *".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(
                err,
                DepError::FonteCaminhoShellRedirection { byte: b'>', .. }
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_backslash_fires_before_shell_glob() {
        // Cascade pin on the upstream backslash arm: a value
        // carrying both `\` and `*` (`"..\caixa-teia\*"` — the
        // canonical "I pasted a Windows-shell `cd ..\path\*` glob
        // expression" footgun) routes through
        // `FonteCaminhoBackslash` not `FonteCaminhoShellGlob`. The
        // cross-host-OS-separator divergence is the load-bearing
        // axis on every probe-as-both value (an author who removes
        // the `\` is the root-cause edit; the `*` falls away in the
        // same edit since it's downstream of the Windows-shell
        // convention).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "..\\caixa-teia\\*".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoBackslash { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_control_char_fires_before_shell_glob() {
        // Cascade pin on the embedded-control-byte arm: a value
        // carrying both a control byte and `*` (`"../foo\n*"` — the
        // canonical paste-from-multiline-doc footgun where a
        // newline landed mid-caminho between two paste fragments)
        // routes through `FonteCaminhoControlChar` not
        // `FonteCaminhoShellGlob`. The POSIX-syscall-rejected-byte /
        // NUL-`CString::new`-fail diagnostic is the load-bearing
        // axis on every value that probes positive for both —
        // mirrors the cascade discipline on every prior arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo\n*".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoControlChar { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_absolute_fires_before_shell_glob() {
        // Cascade pin on the load-bearing leading-byte arm: a
        // leading `/` value with embedded `*` (`"/etc/*"`) routes
        // through `FonteCaminhoAbsolute` not `FonteCaminhoShellGlob`
        // — the host-layout-leak diagnostic is the load-bearing
        // axis, the glob byte is the secondary observation. Same
        // precedence logic as every prior leading-byte arm.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "/etc/*".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoAbsolute { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_glob_fires_before_trailing_slash() {
        // Cascade pin on the immediate-successor arm: a value
        // carrying both `*` and a trailing `/` (`"../foo*/"` — the
        // canonical "I tab-completed a path that already had a
        // glob-expansion tail" footgun) routes through
        // `FonteCaminhoShellGlob` not `FonteCaminhoTrailingSlash`.
        // The embedded shell-metachar is the more semantic-locating
        // axis (an author who removes the `*` typically also drops
        // the trailing separator since both are paste-from-shell
        // artifacts).
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../foo*/".into(),
        });
        let err = d.validate().unwrap_err();
        assert!(
            matches!(err, DepError::FonteCaminhoShellGlob { byte: b'*', .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn fonte_caminho_shell_glob_diagnostic_carries_offending_dep_caminho_and_byte() {
        // Diagnostic-shape pin (peer with
        // `fonte_caminho_shell_redirection_diagnostic_*` on the
        // closest two-byte peer arm): the error's Display surfaces
        // the offending `:nome`, the offending `:caminho` verbatim,
        // the offending byte's hex / character form, and names the
        // shell-glob / pathname-expansion footgun explicitly so a
        // `feira lint` run can render the diagnostic without
        // re-parsing.
        let d = dep_with_fonte(DepSource::Path {
            caminho: "../caixa-teia/*.lisp".into(),
        });
        let rendered = d.validate().unwrap_err().to_string();
        assert!(
            rendered.contains("caixa-teia"),
            "diagnostic must name the offending dep: {rendered}",
        );
        assert!(
            rendered.contains("../caixa-teia/*.lisp"),
            "diagnostic must quote the offending caminho verbatim: {rendered:?}",
        );
        assert!(
            rendered.contains("0x2a"),
            "diagnostic must surface the offending byte hex: {rendered:?}",
        );
        assert!(
            rendered.contains("glob"),
            "diagnostic must name the shell-glob footgun: {rendered:?}",
        );
        assert!(
            rendered.contains("pathname-expansion"),
            "diagnostic must reference the POSIX pathname-expansion vocabulary: {rendered:?}",
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
        // edit. Cover all seven variants so a future variant addition
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
            (
                "caminho-absolute",
                DepSource::Path {
                    caminho: "/home/me/work/caixa-teia".into(),
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
