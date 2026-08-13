use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A recursive attribute value — scalars, lists, objects, and typed refs.
///
/// BTreeMap for objects keeps serialization deterministic.
///
/// The [`gen_platform::IsVariant`] derive emits per-arm arm-discriminator
/// predicates — [`Self::is_str`], [`Self::is_int`], [`Self::is_float`],
/// [`Self::is_bool`], [`Self::is_list`], [`Self::is_object`],
/// [`Self::is_ref`], [`Self::is_null`] — so every downstream consumer that
/// only needs the arm-discriminator projection (not the borrowed field
/// value) reaches for one typed dispatch on the substrate primitive
/// rather than a hand-rolled `matches!(x, TeiaValue::X(_))` literal.
/// Peer of the sibling [`caixa_ast::NodeKind`] (359badc) /
/// [`caixa_core::WitTarget`] (5d776d8-era) / [`caixa_core::CaixaKind`] /
/// [`caixa_core::CaixaDialeto`] / [`caixa_core::DepList`] /
/// [`caixa_core::UpgradeInstruction`] / caixa-ast per-`Trivia`-family /
/// caixa-ast per-`LexToken`-family / caixa-lint / caixa-provedor /
/// caixa-theme sibling closed-set-typed enums that already carry the
/// `gen_platform::IsVariant` discipline — extends the discipline onto
/// the substrate's IaC-side per-`(defteia …)` attribute value sum-type
/// every downstream caixa-arch / caixa-pangea / caixa-teia consumer
/// partitions on. Sibling in shape to the peer per-arm
/// `Option<&<payload>>` projection accessors [`Self::as_str`] (7304ffe)
/// / [`Self::as_object`] (7304ffe) already lifted on this same outer-
/// `TeiaValue` sum-type surface — the arm-discriminator predicate
/// family the projection-accessor family builds against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gen_platform::IsVariant)]
#[serde(untagged)]
pub enum TeiaValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<TeiaValue>),
    Object(BTreeMap<String, TeiaValue>),
    /// A typed reference produced by `(ref aws/vpc main id)`. The renderer
    /// emits `${aws_vpc.main.id}` (Terraform) or the platform equivalent.
    Ref(TeiaRefRepr),
    Null,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeiaRefRepr {
    pub tipo: String,
    pub nome: String,
    pub atributo: String,
}

impl TeiaRefRepr {
    /// Substrate-canonical per-`(ref …)` `:tipo` provider-qualified
    /// resource-type scalar accessor every downstream IaC-facing consumer
    /// of the produced [`TeiaValue::Ref`] carrier keys off — returns the
    /// author-declared `:tipo` byte-string verbatim as a `&str`, borrowed
    /// from the typed slot's own [`String`] storage.
    ///
    /// The `:tipo` slot on the per-`(ref …)` carrier holds the same
    /// qualified `<provider>/<kind>` shape the per-`(defteia …)`
    /// [`crate::TeiaInstance::tipo`] slot carries (the shape
    /// [`crate::manifest::parse_teia_source`] admits at the ref-form
    /// second slot via its [`caixa_ast::NodeKind::Symbol`] projection at
    /// `manifest.rs`:`build_ref`), and every downstream consumer that
    /// reads the identity keys off this scalar (the substrate primitive's
    /// own [`TeiaValue::to_hcl_string`] per-`Ref` `${<tf>.<nome>.<attr>}`
    /// interpolation emit, the `caixa-arch::invariants`
    /// `no-unresolved-refs` per-`Ref` declared-set lookup + refusal
    /// message carrier, the `caixa-pangea::manifest_bridge`
    /// `value_to_json` per-`Ref` Terraform-JSON `${<tf>.<nome>.<attr>}`
    /// interpolation lower — all four production sites across three
    /// crates).
    ///
    /// Prior to this lift the `.tipo` field on the ref carrier was
    /// accessed inline at four caixa-monorepo sites — the substrate
    /// primitive's own [`TeiaValue::to_hcl_string`] `${<tf>.<nome>
    /// .<attr>}` interpolation's `r.tipo.replace('/', "_")` mint (one
    /// site), the `caixa-arch::invariants` `no-unresolved-refs`
    /// declared-set lookup key `(r.tipo.clone(), r.nome.clone())` + the
    /// per-refusal `Violation::message` `(ref <tipo> <nome> <attr>)`
    /// Display interpolation (two sites), and the `caixa-pangea::
    /// manifest_bridge` `InstanceToHcl::mutate` `${<tf>.<nome>.<attr>}`
    /// Terraform-JSON lower's `r.tipo.replace(['/', '-'], "_")` mint (one
    /// site) — each expressed no compile-time link back to the typed
    /// slot. A future extension of the ref carrier's `:tipo` axis to a
    /// richer author surface — a per-provider alias table the substrate
    /// rewrites through the (future) `iac-forge` schema-resolution pass
    /// so a `(ref aws-us-east-1/vpc main id)` provider-region-prefixed
    /// ref emits the correct scoped Terraform-JSON reference block, a
    /// promotion of the `<provider>/<kind>` plain byte-string to the
    /// same scoped-provider-identifier newtype the peer sibling
    /// [`crate::TeiaInstance::tipo`] lifts to on cross-provider
    /// federation, a per-tenant type remap the future M4
    /// `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer resolves per-CR
    /// — would have had to be threaded through every open-coded copy in
    /// lockstep, or the `no-unresolved-refs` declared-set lookup key and
    /// the `caixa-pangea` `${<tf>.<nome>.<attr>}` Terraform-JSON
    /// interpolation would silently disagree on which provider a given
    /// per-`Ref` carrier resolves to; the invariant checker's
    /// `(tipo, nome)` lookup treating the ref target as `"aws/vpc"`
    /// while the Terraform-JSON emit path treated it as
    /// `"tenant-a/aws/vpc"` would silently split the build-time
    /// unresolved-ref refusal from the runtime plan-and-apply pipeline,
    /// one gate disagreeing with the artifact the substrate's teia →
    /// pangea pipeline actually materializes. Lifting the resolution
    /// rule to a typed method on the substrate primitive means every
    /// downstream consumer reaches for exactly one typed dispatch — the
    /// resolver's accept-set migrates as a unit on any future axis
    /// addition.
    ///
    /// First `&str`-return accessor on the outer `TeiaRefRepr` — opens
    /// the outer-`TeiaRefRepr` scalar projection pattern the sibling
    /// `:nome` / `:atributo` future lifts fold on, one axis level down
    /// from the sibling [`crate::TeiaInstance::tipo`] (e58baa7) /
    /// [`crate::TeiaInstance::nome`] (3e2d578) per-`(defteia …)`
    /// primitive accessors that opened + closed the same discipline on
    /// the substrate's IaC-side per-resource-declaration surface. Named
    /// `tipo()` to match the storage field's name — the accessor's
    /// identity name maps onto the author-declared `(ref <tipo> <nome>
    /// <atributo>)` positional-slot vocabulary the parser's own
    /// `build_ref` slot-projection already keys off. Same "one typed
    /// dispatch on the substrate primitive, thin projections at each
    /// consumer" discipline as the sibling [`crate::TeiaInstance::tipo`]
    /// (e58baa7), [`caixa_core::Dep::nome`] (eba2cde), per-`:membros`
    /// [`caixa_core::aplicacao::Membro::nome`] (4a32abf), and outer
    /// [`caixa_core::Caixa::nome`] (e6b7d97 and its convergence family —
    /// most recent 3219a42 / c9be435 / 41ab9a3 / 61d3429) named-caixa-
    /// referencing accessors, extended onto the substrate's IaC-side
    /// per-`(ref …)` reference carrier.
    #[must_use]
    pub fn tipo(&self) -> &str {
        self.tipo.as_str()
    }

    /// Substrate-canonical per-`(ref …)` `:nome` per-reference target-
    /// instance-identity scalar accessor every downstream IaC-facing consumer
    /// of the produced [`TeiaValue::Ref`] carrier keys off — returns the
    /// author-declared `:nome` byte-string verbatim as a `&str`, borrowed
    /// from the typed slot's own [`String`] storage.
    ///
    /// The `:nome` slot on the per-`(ref …)` carrier holds the same per-
    /// resource `main` / `primary` / `worker-a` identity shape the per-
    /// `(defteia …)` [`crate::TeiaInstance::nome`] slot carries (the shape
    /// [`crate::manifest::parse_teia_source`] admits at the ref-form third
    /// slot via its [`caixa_ast::NodeKind::Symbol`] projection at
    /// `manifest.rs`:`build_ref`), and every downstream consumer that reads
    /// the identity keys off this scalar (the substrate primitive's own
    /// [`TeiaValue::to_hcl_string`] per-`Ref` `${<tf>.<nome>.<attr>}`
    /// interpolation emit, the `caixa-arch::invariants` `no-unresolved-refs`
    /// per-`Ref` declared-set lookup key + refusal message `(ref <tipo>
    /// <nome> <attr>)` Display carrier, the `caixa-pangea::manifest_bridge`
    /// `value_to_json` per-`Ref` Terraform-JSON `${<tf>.<nome>.<attr>}`
    /// interpolation lower — all four production sites across three crates).
    ///
    /// Prior to this lift the `.nome` field on the ref carrier was accessed
    /// inline at four caixa-monorepo sites — the substrate primitive's own
    /// [`TeiaValue::to_hcl_string`] `${<tf>.<nome>.<attr>}` interpolation's
    /// `r.nome` emit (one site), the `caixa-arch::invariants`
    /// `no-unresolved-refs` declared-set lookup key `(r.tipo().to_string(),
    /// r.nome.clone())` + the per-refusal `Violation::message` `(ref <tipo>
    /// <nome> <attr>)` Display interpolation (two sites), and the `caixa-
    /// pangea::manifest_bridge` `value_to_json` `${<tf>.<nome>.<attr>}`
    /// Terraform-JSON lower's `r.nome` emit (one site) — each expressed no
    /// compile-time link back to the typed slot. A future extension of the
    /// ref carrier's `:nome` axis to a richer author surface — a promotion
    /// of the plain byte-string to the same DNS-1123-label /
    /// provider-scoped-identifier newtype the peer sibling
    /// [`crate::TeiaInstance::nome`] lifts to once the substrate's IaC-side
    /// identity discipline converges with the CAIXA-SDLC-side
    /// [`caixa_core::Caixa::nome`] shape, a per-tenant `<nome>` rewrite the
    /// future M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR materializer
    /// resolves per-CR so a `(ref aws/vpc main id)` reference target
    /// resolves to `tenant-a-main` at emit time, a per-provider `<nome>`
    /// sanitizer the future `iac-forge` schema-resolution pass routes
    /// through — would have had to be threaded through every open-coded
    /// copy in lockstep, or the `no-unresolved-refs` declared-set lookup
    /// key and the `caixa-pangea` `${<tf>.<nome>.<attr>}` Terraform-JSON
    /// interpolation would silently disagree on which per-`(defteia …)`
    /// target a given `(ref …)` carrier resolves to; the invariant
    /// checker's `(tipo, nome)` lookup treating the target as `"main"`
    /// while the Terraform-JSON emit path treated it as `"tenant-a-main"`
    /// would silently split the build-time unresolved-ref refusal from the
    /// runtime plan-and-apply pipeline, one gate disagreeing with the
    /// artifact the substrate's teia → pangea pipeline actually
    /// materializes. Lifting the resolution rule to a typed method on the
    /// substrate primitive means every downstream consumer reaches for
    /// exactly one typed dispatch — the resolver's accept-set migrates as
    /// a unit on any future axis addition.
    ///
    /// Second `&str`-return accessor on the outer `TeiaRefRepr` — folds on
    /// the outer-`TeiaRefRepr` scalar projection pattern the sibling
    /// [`Self::tipo`] accessor (a856d67) opened, and closes the two
    /// author-declared universal-axis identity-scalar slots (`:tipo` /
    /// `:nome`) on the substrate's IaC-side per-`(ref …)` reference
    /// carrier — one axis level down from the sibling
    /// [`crate::TeiaInstance::tipo`] (e58baa7) / [`crate::TeiaInstance::nome`]
    /// (3e2d578) per-`(defteia …)` primitive accessors that opened +
    /// closed the same discipline on the substrate's IaC-side per-
    /// resource-declaration surface, and mirrors the identical opener +
    /// closer discipline the sibling per-`(ref …)` `:tipo` accessor
    /// (a856d67) opened on the outer-`TeiaRefRepr` type one axis level up.
    /// The `:atributo` slot remains as a future lift; folding it on the
    /// same shape at exactly one call site closes the outer-`TeiaRefRepr`
    /// universal-axis sub-family. Named `nome()` to match the storage
    /// field's name — the accessor's identity name maps onto the author-
    /// declared `(ref <tipo> <nome> <atributo>)` positional-slot
    /// vocabulary the parser's own `build_ref` slot-projection already
    /// keys off. Same "one typed dispatch on the substrate primitive,
    /// thin projections at each consumer" discipline as the sibling
    /// [`Self::tipo`] (a856d67), [`crate::TeiaInstance::tipo`] (e58baa7),
    /// [`crate::TeiaInstance::nome`] (3e2d578), [`caixa_core::Dep::nome`]
    /// (eba2cde), per-`:membros` [`caixa_core::aplicacao::Membro::nome`]
    /// (4a32abf), and outer [`caixa_core::Caixa::nome`] (e6b7d97 and its
    /// convergence family — most recent 3219a42 / c9be435 / 41ab9a3 /
    /// 61d3429) named-caixa-referencing accessors, extended onto the
    /// substrate's IaC-side per-`(ref …)` reference-target-identity axis.
    #[must_use]
    pub fn nome(&self) -> &str {
        self.nome.as_str()
    }

    /// Substrate-canonical per-`(ref …)` `:atributo` per-reference
    /// target-attribute-projection scalar accessor every downstream
    /// IaC-facing consumer of the produced [`TeiaValue::Ref`] carrier
    /// keys off — returns the author-declared `:atributo` byte-string
    /// verbatim as a `&str`, borrowed from the typed slot's own
    /// [`String`] storage.
    ///
    /// The `:atributo` slot on the per-`(ref …)` carrier holds the
    /// per-target attribute projection (`id`, `arn`, `vpc_id`, whatever
    /// the target [`crate::TeiaInstance`]'s provider exposes on the
    /// referenced resource) — the shape [`crate::manifest::parse_teia_source`]
    /// admits at the ref-form fourth slot via its [`caixa_ast::NodeKind::Symbol`]
    /// projection at `manifest.rs`:`build_ref` — and every downstream
    /// consumer that reads the attribute projection keys off this scalar
    /// (the substrate primitive's own [`TeiaValue::to_hcl_string`]
    /// per-`Ref` `${<tf>.<nome>.<attr>}` HCL interpolation emit, the
    /// `caixa-arch::invariants` `no-unresolved-refs` per-`Ref` refusal
    /// `(ref <tipo> <nome> <attr>)` Display interpolation, the
    /// `caixa-pangea::manifest_bridge` `value_to_json` per-`Ref`
    /// Terraform-JSON `${<tf>.<nome>.<attr>}` interpolation lower — all
    /// three production sites across three crates).
    ///
    /// Prior to this lift the `.atributo` field on the ref carrier was
    /// accessed inline at three caixa-monorepo sites — the substrate
    /// primitive's own [`TeiaValue::to_hcl_string`] `${<tf>.<nome>.<attr>}`
    /// interpolation's `r.atributo` emit (one site), the
    /// `caixa-arch::invariants` `no-unresolved-refs` per-refusal
    /// `Violation::message` `(ref <tipo> <nome> <attr>)` Display
    /// interpolation (one site), and the `caixa-pangea::manifest_bridge`
    /// `value_to_json` `${<tf>.<nome>.<attr>}` Terraform-JSON lower's
    /// `r.atributo` emit (one site) — each expressed no compile-time
    /// link back to the typed slot. A future extension of the ref
    /// carrier's `:atributo` axis to a richer author surface — a
    /// per-provider attribute-alias table the future `iac-forge`
    /// schema-resolution pass routes through so a `(ref aws/vpc main
    /// cidr-block)` reference emits the provider-canonical
    /// `cidr_block` at Terraform-JSON emit time, a per-tenant
    /// attribute-rewrite the future M4 `mesh.pleme.io/v1alpha1/Aplicacao`
    /// CR materializer resolves per-CR, a promotion of the plain
    /// byte-string to a typed provider-scoped attribute-name newtype
    /// once the substrate's IaC-side attribute discipline converges
    /// with the CAIXA-SDLC-side identity shape — would have had to be
    /// threaded through every open-coded copy in lockstep, or the
    /// `no-unresolved-refs` refusal Display interpolation and the
    /// `caixa-pangea` `${<tf>.<nome>.<attr>}` Terraform-JSON
    /// interpolation would silently disagree on which per-target
    /// attribute a given `(ref …)` carrier projects to; the invariant
    /// checker's refusal message reading `id` while the Terraform-JSON
    /// emit path silently read `arn` would silently split the
    /// build-time unresolved-ref refusal diagnostic from the runtime
    /// plan-and-apply pipeline, one gate disagreeing with the artifact
    /// the substrate's teia → pangea pipeline actually materializes.
    /// Lifting the resolution rule to a typed method on the substrate
    /// primitive means every downstream consumer reaches for exactly
    /// one typed dispatch — the resolver's accept-set migrates as a
    /// unit on any future axis addition.
    ///
    /// Third and final `&str`-return accessor on the outer `TeiaRefRepr`
    /// — folds on the outer-`TeiaRefRepr` scalar projection pattern the
    /// sibling [`Self::tipo`] (a856d67) accessor opened and the sibling
    /// [`Self::nome`] (15bcdef) accessor extended, and closes the
    /// outer-`TeiaRefRepr` universal-axis scalar sub-family across all
    /// three positional slots (`:tipo` / `:nome` / `:atributo`) the
    /// parser's own `build_ref` slot-projection admits. Same "one
    /// typed dispatch on the substrate primitive, thin projections at
    /// each consumer" discipline as the sibling [`Self::tipo`]
    /// (a856d67), [`Self::nome`] (15bcdef), [`crate::TeiaInstance::tipo`]
    /// (e58baa7), [`crate::TeiaInstance::nome`] (3e2d578),
    /// [`caixa_core::Dep::nome`] (eba2cde), per-`:membros`
    /// [`caixa_core::aplicacao::Membro::nome`] (4a32abf), and outer
    /// [`caixa_core::Caixa::nome`] (e6b7d97 and its convergence family)
    /// accessors, extended onto the substrate's IaC-side per-`(ref …)`
    /// reference-target-attribute-projection axis. Named `atributo()`
    /// to match the storage field's name — the accessor's identity
    /// name maps onto the author-declared `(ref <tipo> <nome>
    /// <atributo>)` positional-slot vocabulary the parser's own
    /// `build_ref` slot-projection already keys off.
    #[must_use]
    pub fn atributo(&self) -> &str {
        self.atributo.as_str()
    }
}

impl TeiaValue {
    /// Substrate-canonical projection onto the [`Self::Str`] arm's
    /// borrowed scalar payload — returns `Some(&str)` byte-borrowed
    /// from the arm's own [`String`] storage, and [`None`] on every
    /// other arm of the closed 8-arm [`TeiaValue`] variant set.
    ///
    /// One production consumer today across the substrate — the
    /// `caixa-arch::invariants` `cidr-block-looks-valid` per-
    /// [`TeiaInstance`]-attribute `:cidr-block` string-arm projection
    /// at [`caixa_arch::invariants`]::`cidr_block_format_hint`
    /// (`invariants.rs`:249) — which previously reached the
    /// underlying scalar through a raw
    /// `if let Some(TeiaValue::Str(s)) = inst.atributos.get(...)`
    /// open-coded per-arm pattern-match that expressed no compile-
    /// time link back to the substrate primitive's typed scalar-arm
    /// projection. A future `TeiaValue` variant addition (the
    /// per-provider tagged-scalar shape the substrate's IaC-side
    /// scalar-projection axis may grow once `iac-forge` lifts a
    /// per-provider scoped scalar tier, an `Enum(String)` variant
    /// the per-provider option-list shape converges onto once the
    /// M4 typed-registry lands) reaches every downstream per-
    /// [`Self::Str`]-arm consumer through this one dispatch by
    /// construction — no coordinated N-way rewrite across every
    /// per-consumer projection site.
    ///
    /// Zero-copy — the returned `&str` borrows from the arm's own
    /// [`String`] storage (pinned by the `as_str_is_by_borrow_pointer_
    /// identity` test), the same discipline as the sibling
    /// [`TeiaRefRepr::tipo`] (a856d67) / [`TeiaRefRepr::nome`]
    /// (15bcdef) / [`TeiaRefRepr::atributo`] outer-carrier scalar
    /// accessors on the substrate's IaC-side per-`(ref …)` reference
    /// carrier — one axis level up from those sibling
    /// outer-`TeiaRefRepr` scalar accessors, extended onto the
    /// outer-`TeiaValue` sum-type projection surface.
    ///
    /// First `Option<&<payload>>` projection accessor on the outer
    /// [`TeiaValue`] sum-type — opens the `as_<variant>` typed
    /// projection family the sibling [`Self::as_object`] paired lift
    /// closes on the two `caixa-arch::invariants` consumer sites, and
    /// the future per-arm [`Self::as_list`] / [`Self::as_int`] /
    /// [`Self::as_float`] / [`Self::as_bool`] / [`Self::as_ref`]
    /// projections fold on the same shape at their first cross-crate
    /// consumer.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Substrate-canonical projection onto the [`Self::Object`] arm's
    /// borrowed [`BTreeMap`] payload — returns `Some(&BTreeMap<…>)`
    /// borrowed from the arm's own map storage, and [`None`] on every
    /// other arm of the closed 8-arm [`TeiaValue`] variant set.
    ///
    /// One production consumer today across the substrate — the
    /// `caixa-arch::invariants` `no-public-ingress-without-tags`
    /// per-[`TeiaInstance`]-attribute `:tags` object-arm projection at
    /// [`caixa_arch::invariants`]::`no_public_ingress_without_tags`
    /// (`invariants.rs`:222–225) — which previously reached the
    /// underlying map through a raw
    /// `.and_then(|v| match v { TeiaValue::Object(m) => Some(m),
    /// _ => None })` inline `.and_then` closure that expressed no
    /// compile-time link back to the substrate primitive's typed
    /// object-arm projection. Sibling in shape to [`Self::as_str`]:
    /// the same "one canonical projection dispatch per typed variant
    /// arm on the substrate primitive, thin `.and_then(TeiaValue::as_*)`
    /// per-consumer call" discipline the sibling scalar projection
    /// [`Self::as_str`] opens.
    ///
    /// Zero-copy — the returned reference borrows the arm's own
    /// [`BTreeMap`] storage (pinned by the
    /// `as_object_is_by_borrow_pointer_identity` test), no clone.
    ///
    /// Paired closer with [`Self::as_str`] on the two `caixa-arch::
    /// invariants` per-`TeiaValue`-arm consumer projection sites —
    /// every open-coded outer-`TeiaValue` sum-type per-arm projection
    /// across the substrate now routes through exactly one
    /// per-variant `as_*` accessor, so a future arm addition reaches
    /// every downstream consumer through construction rather than a
    /// coordinated per-site rewrite. Same discipline extension the
    /// sibling [`caixa_core::WitTarget::payload`] (5d6dc92) /
    /// [`caixa_core::WitTarget::http_endpoint`] (dd83cf1) per-arm
    /// projection accessors carried onto the M3 `:contratos` sum-type
    /// dispatch surface, extended here onto the substrate's IaC-side
    /// per-`(defteia …)` attribute value sum-type.
    #[must_use]
    pub fn as_object(&self) -> Option<&BTreeMap<String, TeiaValue>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }

    #[must_use]
    pub fn to_hcl_string(&self) -> String {
        match self {
            Self::Str(s) => format!("{s:?}"),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Null => "null".to_string(),
            Self::List(items) => {
                let parts: Vec<String> = items.iter().map(Self::to_hcl_string).collect();
                format!("[{}]", parts.join(", "))
            }
            Self::Object(map) => {
                let parts: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{k} = {}", v.to_hcl_string()))
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            }
            Self::Ref(r) => {
                // Route the internal `<provider>_<kind>` HCL-string type-
                // name mint through the lifted [`TeiaRefRepr::tipo`]
                // scalar accessor rather than the raw `r.tipo` field
                // access — the substrate primitive's own
                // `${<tf>.<nome>.<attr>}` per-`Ref` interpolation emit
                // path now keys off the canonical raw-slot surface every
                // downstream `caixa-arch` / `caixa-pangea` per-
                // `TeiaRefRepr` `:tipo` consumer routes through, so any
                // future rebrand on the typed slot's raw-slot reader
                // lands at exactly one place.
                let tipo = r.tipo().replace('/', "_");
                // Same discipline for the per-`Ref` `<nome>` and
                // `<attr>` carries — route through the lifted
                // [`TeiaRefRepr::nome`] / [`TeiaRefRepr::atributo`]
                // scalar accessors rather than the raw `r.nome` /
                // `r.atributo` field accesses. The substrate
                // primitive's own `${<tf>.<nome>.<attr>}` per-`Ref`
                // interpolation emit path now keys off the canonical
                // raw-slot surface every downstream `caixa-arch` /
                // `caixa-pangea` per-`TeiaRefRepr` `:nome` / `:atributo`
                // consumer routes through, so any future rebrand on
                // either typed slot's raw-slot reader lands at exactly
                // one place.
                format!("${{{tipo}.{}.{}}}", r.nome(), r.atributo())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipo_returns_declared_tipo_verbatim() {
        // The accessor is a projection, not a gate. Every author-declared
        // per-`(ref …)` `:tipo` byte-string — the canonical `aws/vpc`
        // shape, a hyphenated `google/compute-instance` shape, a snake_
        // cased `aws_iam_role` shape (already-normalized), a namespaced
        // `akeyless/static-secret`, an empty `""` sentinel — round-trips
        // as-is through `TeiaRefRepr::tipo()`. Pins the "verbatim
        // projection" contract every downstream consumer (`caixa-arch::
        // invariants` `no-unresolved-refs` declared-set lookup + Display
        // interpolation, `caixa-pangea::manifest_bridge` `<provider>_
        // <kind>` mint, `TeiaValue::to_hcl_string` per-`Ref`
        // interpolation) depends on.
        for fixture in [
            "aws/vpc",
            "akeyless/static-secret",
            "google/compute-instance",
            "aws_iam_role",
            "",
        ] {
            let r = TeiaRefRepr {
                tipo: fixture.into(),
                nome: "main".into(),
                atributo: "id".into(),
            };
            assert_eq!(
                r.tipo(),
                fixture,
                "TeiaRefRepr::tipo must return the author-declared :tipo \
                 byte-string verbatim (fixture: {fixture:?})",
            );
        }
    }

    #[test]
    fn tipo_is_by_borrow_pointer_identity() {
        // Zero-copy pin — `r.tipo()` must borrow from the typed slot's
        // own [`String`] storage, not clone into a fresh buffer. Fails at
        // build time if a future rewrite regresses to an owned-buffer
        // shape (`self.tipo.clone()` in the body would type-check but
        // silently allocate on every call — the pointer identity check
        // catches it).
        let r = TeiaRefRepr {
            tipo: "aws/vpc".into(),
            nome: "main".into(),
            atributo: "id".into(),
        };
        let via_accessor: &str = r.tipo();
        assert_eq!(
            via_accessor.as_ptr(),
            r.tipo.as_ptr(),
            "TeiaRefRepr::tipo must borrow from the .tipo String's \
             backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            r.tipo.len(),
            "TeiaRefRepr::tipo and .tipo.as_str() must byte-equal in \
             length (same slice)",
        );
    }

    #[test]
    fn nome_returns_declared_nome_verbatim() {
        // The accessor is a projection, not a gate. Every author-declared
        // per-`(ref …)` `:nome` byte-string — the canonical `main` shape,
        // a hyphenated `worker-a` shape, a namespaced-ish `primary-01`
        // shape, a digit-prefixed `v2-worker` shape, an empty `""`
        // sentinel — round-trips as-is through `TeiaRefRepr::nome()`.
        // Pins the "verbatim projection" contract every downstream
        // consumer (`caixa-arch::invariants` `no-unresolved-refs` per-
        // `Ref` declared-set lookup key + `(ref <tipo> <nome> <attr>)`
        // Display interpolation, `caixa-pangea::manifest_bridge` per-`Ref`
        // `${<tf>.<nome>.<attr>}` Terraform-JSON interpolation, the
        // substrate primitive's own `TeiaValue::to_hcl_string` per-`Ref`
        // `${<tf>.<nome>.<attr>}` HCL interpolation) depends on.
        for fixture in ["main", "worker-a", "primary-01", "v2-worker", ""] {
            let r = TeiaRefRepr {
                tipo: "aws/vpc".into(),
                nome: fixture.into(),
                atributo: "id".into(),
            };
            assert_eq!(
                r.nome(),
                fixture,
                "TeiaRefRepr::nome must return the author-declared :nome \
                 byte-string verbatim (fixture: {fixture:?})",
            );
        }
    }

    #[test]
    fn nome_is_by_borrow_pointer_identity() {
        // Zero-copy pin — `r.nome()` must borrow from the typed slot's
        // own [`String`] storage, not clone into a fresh buffer. Fails at
        // build time if a future rewrite regresses to an owned-buffer
        // shape (`self.nome.clone()` in the body would type-check but
        // silently allocate on every call — the pointer identity check
        // catches it).
        let r = TeiaRefRepr {
            tipo: "aws/vpc".into(),
            nome: "main".into(),
            atributo: "id".into(),
        };
        let via_accessor: &str = r.nome();
        assert_eq!(
            via_accessor.as_ptr(),
            r.nome.as_ptr(),
            "TeiaRefRepr::nome must borrow from the .nome String's \
             backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            r.nome.len(),
            "TeiaRefRepr::nome and .nome.as_str() must byte-equal in \
             length (same slice)",
        );
    }

    #[test]
    fn to_hcl_string_reader_routes_through_nome_accessor() {
        // Coherence pin: the substrate primitive's own
        // `TeiaValue::to_hcl_string` per-`Ref` interpolation reader and
        // every downstream `caixa-arch` / `caixa-pangea` per-`:nome`
        // consumer share the same accessor. Regresses if a future detour
        // re-inlines the raw `r.nome` field access at the
        // `${<tf>.<nome>.<attr>}` interpolation emit path.
        let v = TeiaValue::Ref(TeiaRefRepr {
            tipo: "aws/vpc".into(),
            nome: "primary".into(),
            atributo: "id".into(),
        });
        assert_eq!(
            v.to_hcl_string(),
            "${aws_vpc.primary.id}",
            "to_hcl_string must carry <nome> through the \
             TeiaRefRepr::nome() accessor",
        );
    }

    #[test]
    fn atributo_returns_declared_atributo_verbatim() {
        // The accessor is a projection, not a gate. Every author-declared
        // per-`(ref …)` `:atributo` byte-string — the canonical `id`
        // shape, a snake_cased `vpc_id`, a kebab-cased `cidr-block`, a
        // provider-scoped `arn`, an empty `""` sentinel — round-trips
        // as-is through `TeiaRefRepr::atributo()`. Pins the "verbatim
        // projection" contract every downstream consumer (`caixa-arch::
        // invariants` `no-unresolved-refs` per-`Ref` refusal
        // `(ref <tipo> <nome> <attr>)` Display interpolation,
        // `caixa-pangea::manifest_bridge` per-`Ref`
        // `${<tf>.<nome>.<attr>}` Terraform-JSON interpolation, the
        // substrate primitive's own `TeiaValue::to_hcl_string` per-`Ref`
        // `${<tf>.<nome>.<attr>}` HCL interpolation) depends on.
        for fixture in ["id", "vpc_id", "cidr-block", "arn", ""] {
            let r = TeiaRefRepr {
                tipo: "aws/vpc".into(),
                nome: "main".into(),
                atributo: fixture.into(),
            };
            assert_eq!(
                r.atributo(),
                fixture,
                "TeiaRefRepr::atributo must return the author-declared \
                 :atributo byte-string verbatim (fixture: {fixture:?})",
            );
        }
    }

    #[test]
    fn atributo_is_by_borrow_pointer_identity() {
        // Zero-copy pin — `r.atributo()` must borrow from the typed
        // slot's own [`String`] storage, not clone into a fresh buffer.
        // Fails at build time if a future rewrite regresses to an
        // owned-buffer shape (`self.atributo.clone()` in the body would
        // type-check but silently allocate on every call — the pointer
        // identity check catches it).
        let r = TeiaRefRepr {
            tipo: "aws/vpc".into(),
            nome: "main".into(),
            atributo: "id".into(),
        };
        let via_accessor: &str = r.atributo();
        assert_eq!(
            via_accessor.as_ptr(),
            r.atributo.as_ptr(),
            "TeiaRefRepr::atributo must borrow from the .atributo \
             String's backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            r.atributo.len(),
            "TeiaRefRepr::atributo and .atributo.as_str() must \
             byte-equal in length (same slice)",
        );
    }

    #[test]
    fn to_hcl_string_reader_routes_through_atributo_accessor() {
        // Coherence pin: the substrate primitive's own
        // `TeiaValue::to_hcl_string` per-`Ref` interpolation reader and
        // every downstream `caixa-arch` / `caixa-pangea` per-`:atributo`
        // consumer share the same accessor. Regresses if a future
        // detour re-inlines the raw `r.atributo` field access at the
        // `${<tf>.<nome>.<attr>}` interpolation emit path — this
        // fixture pins the `<attr>` slot as `cidr-block` (a
        // hyphenated attribute name preserved verbatim by the
        // projection, unlike the `<tipo>` slot's `/` → `_` mint) so a
        // future rewrite that accidentally sanitizes the attribute
        // through the type-name mint pipeline surfaces at build time.
        let v = TeiaValue::Ref(TeiaRefRepr {
            tipo: "aws/vpc".into(),
            nome: "main".into(),
            atributo: "cidr-block".into(),
        });
        assert_eq!(
            v.to_hcl_string(),
            "${aws_vpc.main.cidr-block}",
            "to_hcl_string must carry <attr> through the \
             TeiaRefRepr::atributo() accessor verbatim (no sanitizer)",
        );
    }

    #[test]
    fn as_str_projects_only_str_arm() {
        // Projection contract on the outer-`TeiaValue` sum-type's
        // `:Str` scalar-arm accessor: exactly the `Str` arm returns
        // `Some(&str)` byte-borrowed from the arm's own [`String`]
        // storage; every other arm returns `None`. Pins the "one
        // canonical projection dispatch per typed arm on the
        // substrate primitive" discipline the two `caixa-arch::
        // invariants` per-`TeiaValue`-arm consumer sites route
        // through via `.and_then(TeiaValue::as_str)`.
        //
        // The `caixa-arch::invariants` `cidr-block-looks-valid`
        // per-`:cidr-block` string-arm projection at
        // `cidr_block_format_hint` is the sole production consumer
        // today; a regression that admitted an `Object` / `List` /
        // `Int` / `Bool` / `Float` / `Null` / `Ref` arm through the
        // scalar-arm projection would silently classify the non-
        // string attribute as a string at the invariant gate and
        // trip a spurious hint. This test guards that surface.
        assert_eq!(
            TeiaValue::Str("10.0.0.0/16".into()).as_str(),
            Some("10.0.0.0/16")
        );
        assert_eq!(TeiaValue::Str(String::new()).as_str(), Some(""));
        assert_eq!(TeiaValue::Int(42).as_str(), None);
        assert_eq!(TeiaValue::Float(2.5).as_str(), None);
        assert_eq!(TeiaValue::Bool(true).as_str(), None);
        assert_eq!(TeiaValue::Null.as_str(), None);
        assert_eq!(TeiaValue::List(vec![TeiaValue::Int(1)]).as_str(), None);
        let mut m = BTreeMap::new();
        m.insert("k".into(), TeiaValue::Str("v".into()));
        assert_eq!(TeiaValue::Object(m).as_str(), None);
        assert_eq!(
            TeiaValue::Ref(TeiaRefRepr {
                tipo: "aws/vpc".into(),
                nome: "main".into(),
                atributo: "id".into(),
            })
            .as_str(),
            None,
        );
    }

    #[test]
    fn as_str_is_by_borrow_pointer_identity() {
        // Zero-copy pin — `v.as_str()` must borrow from the `Str`
        // arm's own [`String`] storage, not clone into a fresh
        // buffer. Fails at build time if a future rewrite regresses
        // to `Some(s.clone().leak())` or any other detour that
        // silently allocates on every call (the same shape as the
        // sibling [`TeiaRefRepr::tipo_is_by_borrow_pointer_identity`]
        // pin on the outer-`TeiaRefRepr` scalar accessor).
        let v = TeiaValue::Str("primary-01".into());
        let via_accessor: &str = v.as_str().unwrap();
        let TeiaValue::Str(ref inner) = v else {
            unreachable!("constructed above as TeiaValue::Str");
        };
        assert_eq!(
            via_accessor.as_ptr(),
            inner.as_ptr(),
            "TeiaValue::as_str must borrow from the Str arm's String \
             backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            inner.len(),
            "TeiaValue::as_str and inner.as_str() must byte-equal in \
             length (same slice)",
        );
    }

    #[test]
    fn as_object_projects_only_object_arm() {
        // Sibling projection contract to
        // `as_str_projects_only_str_arm`: exactly the `Object` arm
        // returns `Some(&BTreeMap<…>)` borrowed from the arm's own
        // map storage; every other arm returns `None`. Pins the
        // per-`Object`-arm projection the `caixa-arch::invariants`
        // `no-public-ingress-without-tags` per-`:tags` consumer at
        // `no_public_ingress_without_tags` routes through via
        // `.and_then(TeiaValue::as_object)`; a regression that
        // admitted a non-`Object` arm through this projection would
        // silently treat any per-attribute scalar or list as a tag
        // map and lose the "public ingress needs :owner/:team"
        // enforcement.
        let mut m = BTreeMap::new();
        m.insert("owner".into(), TeiaValue::Str("team-a".into()));
        let obj = TeiaValue::Object(m.clone());
        assert_eq!(obj.as_object(), Some(&m));
        assert_eq!(
            TeiaValue::Object(BTreeMap::new()).as_object(),
            Some(&BTreeMap::new())
        );
        assert_eq!(TeiaValue::Str("main".into()).as_object(), None);
        assert_eq!(TeiaValue::Int(0).as_object(), None);
        assert_eq!(TeiaValue::Float(0.0).as_object(), None);
        assert_eq!(TeiaValue::Bool(false).as_object(), None);
        assert_eq!(TeiaValue::Null.as_object(), None);
        assert_eq!(TeiaValue::List(Vec::new()).as_object(), None);
        assert_eq!(
            TeiaValue::Ref(TeiaRefRepr {
                tipo: "aws/vpc".into(),
                nome: "main".into(),
                atributo: "id".into(),
            })
            .as_object(),
            None,
        );
    }

    #[test]
    fn as_object_is_by_borrow_pointer_identity() {
        // Zero-copy pin — `v.as_object()` must borrow from the
        // `Object` arm's own [`BTreeMap`] storage, not clone into a
        // fresh map. Fails at build time if a future rewrite regresses
        // to `Some(map.clone())` (which would type-check with an owned
        // `BTreeMap` return but silently allocate per call).
        let mut m = BTreeMap::new();
        m.insert("dono".into(), TeiaValue::Str("infra".into()));
        let v = TeiaValue::Object(m);
        let via_accessor = v.as_object().unwrap();
        let TeiaValue::Object(ref inner) = v else {
            unreachable!("constructed above as TeiaValue::Object");
        };
        assert!(
            std::ptr::eq(via_accessor, inner),
            "TeiaValue::as_object must borrow from the Object arm's \
             BTreeMap backing storage (zero-copy projection)",
        );
    }

    fn all_teia_value_variants() -> Vec<(TeiaValue, &'static str)> {
        let mut m = BTreeMap::new();
        m.insert("k".into(), TeiaValue::Str("v".into()));
        vec![
            (TeiaValue::Str("s".into()), "Str"),
            (TeiaValue::Int(0), "Int"),
            (TeiaValue::Float(0.0), "Float"),
            (TeiaValue::Bool(false), "Bool"),
            (TeiaValue::List(Vec::new()), "List"),
            (TeiaValue::Object(m), "Object"),
            (
                TeiaValue::Ref(TeiaRefRepr {
                    tipo: "aws/vpc".into(),
                    nome: "main".into(),
                    atributo: "id".into(),
                }),
                "Ref",
            ),
            (TeiaValue::Null, "Null"),
        ]
    }

    fn predicate_row(v: &TeiaValue) -> [bool; 8] {
        [
            v.is_str(),
            v.is_int(),
            v.is_float(),
            v.is_bool(),
            v.is_list(),
            v.is_object(),
            v.is_ref(),
            v.is_null(),
        ]
    }

    // Fail-before-pass-after pin on the [`gen_platform::IsVariant`]
    // derive-generated per-arm predicate partition — for every variant
    // in `all_teia_value_variants()`, the observed 8-slot predicate row
    // must equal a one-hot row with the `true` at exactly the same index
    // as the variant's declaration order. Expected rows are generated
    // live from the enumeration rather than transcribed by hand, so a
    // copy-paste flip that reroutes one arm through the wrong predicate
    // lane trips at the identity-diagonal assertion the way every peer
    // `caixa_ast::NodeKind` / `caixa_ast::TriviaKind` /
    // `caixa_ast::LexToken` / `caixa_core::CaixaKind` /
    // `caixa_core::CaixaDialeto` / `caixa_core::DepList` /
    // `caixa_core::PathShapeViolation` / `caixa_core::RestartStrategy`
    // / `caixa_core::WitTarget` partition pin already does on the
    // sibling caixa-ast / caixa-core surfaces.
    #[test]
    fn teia_value_is_variant_predicates_partition_the_arm_set() {
        let variants = all_teia_value_variants();
        for (idx, (variant, name)) in variants.iter().enumerate() {
            let observed = predicate_row(variant);
            let mut expected = [false; 8];
            expected[idx] = true;
            assert_eq!(
                observed, expected,
                "TeiaValue::{name} at declaration-order slot {idx} must \
                 satisfy exactly one is_* predicate (its own); observed \
                 row must equal the one-hot expected row"
            );
        }
    }

    // Byte-parity pin on the two field-agnostic `matches!` shapes the
    // per-consumer converge in this run replaces at the caixa-teia
    // manifest-test surface: the `TeiaValue::Object(_)` gate at the
    // `is_kwargs_predicate_keys_off_is_keyword_derived_arm_check` per-
    // `:tags` shape assertion (caixa-teia/src/manifest.rs) and the
    // `TeiaValue::List(_)` gate at the sibling per-`:things` shape
    // assertion. Refuses a future accidental split between the derived
    // predicate and its pre-lift `matches!` shape (a hand-rolled shadow
    // `impl` that overrides one path, an accidental rebrand of one
    // converged call site back to the `matches!` form) on the two
    // load-bearing arm-discriminator axes the manifest-lowerer's
    // per-attribute shape gate partitions on.
    #[test]
    fn teia_value_is_object_and_is_list_byte_equal_pre_lift_matches_shape() {
        for (variant, name) in all_teia_value_variants() {
            let via_matches_object = matches!(variant, TeiaValue::Object(_));
            let via_predicate_object = variant.is_object();
            assert_eq!(
                via_predicate_object, via_matches_object,
                "TeiaValue::{name}.is_object() must byte-equal \
                 matches!(_, TeiaValue::Object(_)) — otherwise the \
                 converged call site in caixa-teia manifest tests would \
                 silently disagree with its pre-lift shape"
            );
            let via_matches_list = matches!(variant, TeiaValue::List(_));
            let via_predicate_list = variant.is_list();
            assert_eq!(
                via_predicate_list, via_matches_list,
                "TeiaValue::{name}.is_list() must byte-equal \
                 matches!(_, TeiaValue::List(_)) — otherwise the \
                 converged call site in caixa-teia manifest tests would \
                 silently disagree with its pre-lift shape"
            );
        }
    }

    #[test]
    fn to_hcl_string_reader_routes_through_tipo_accessor() {
        // Coherence pin: the substrate primitive's own
        // `TeiaValue::to_hcl_string` per-`Ref` interpolation reader and
        // every downstream `caixa-arch` / `caixa-pangea` per-`:tipo`
        // consumer share the same accessor. Regresses if a future detour
        // re-inlines the raw `r.tipo` field access at the
        // `${<tf>.<nome>.<attr>}` interpolation emit path.
        let v = TeiaValue::Ref(TeiaRefRepr {
            tipo: "aws/vpc".into(),
            nome: "main".into(),
            atributo: "id".into(),
        });
        assert_eq!(
            v.to_hcl_string(),
            "${aws_vpc.main.id}",
            "to_hcl_string must mint <provider>_<kind> through the \
             TeiaRefRepr::tipo() accessor",
        );
    }
}
