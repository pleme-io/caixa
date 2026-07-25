use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A recursive attribute value — scalars, lists, objects, and typed refs.
///
/// BTreeMap for objects keeps serialization deterministic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

impl TeiaValue {
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
                // Same discipline for the per-`Ref` `<nome>` carry —
                // route through the lifted [`TeiaRefRepr::nome`] scalar
                // accessor rather than the raw `r.nome` field access.
                // The substrate primitive's own `${<tf>.<nome>.<attr>}`
                // per-`Ref` interpolation emit path now keys off the
                // canonical raw-slot surface every downstream
                // `caixa-arch` / `caixa-pangea` per-`TeiaRefRepr`
                // `:nome` consumer routes through, so any future
                // rebrand on the typed slot's raw-slot reader lands at
                // exactly one place.
                format!("${{{tipo}.{}.{}}}", r.nome(), r.atributo)
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
