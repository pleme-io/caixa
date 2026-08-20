use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::value::TeiaValue;

/// One resource instance — the runtime result of compiling a `(defteia …)`
/// form. Rendered by backends into HCL / Ruby / Lisp / Go provider code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeiaInstance {
    /// Qualified resource type — e.g. `aws/vpc`, `akeyless/secret`.
    pub tipo: String,
    /// Instance name — e.g. `main`, `primary`.
    pub nome: String,
    /// Attribute values, keyed by attribute name.
    #[serde(default)]
    pub atributos: BTreeMap<String, TeiaValue>,
}

impl TeiaInstance {
    #[must_use]
    pub fn new(tipo: impl Into<String>, nome: impl Into<String>) -> Self {
        Self {
            tipo: tipo.into(),
            nome: nome.into(),
            atributos: BTreeMap::new(),
        }
    }

    /// Substrate-canonical per-`(defteia …)` `:tipo` provider-qualified
    /// resource-type scalar accessor every downstream IaC-facing consumer
    /// of the `TeiaInstance` primitive keys off — returns the author-
    /// declared `:tipo` byte-string verbatim as a `&str`, borrowed from
    /// the typed slot's own [`String`] storage.
    ///
    /// The `:tipo` slot carries the qualified `<provider>/<kind>` shape
    /// (`aws/vpc`, `akeyless/secret`, `google/compute-instance` — the
    /// shape [`crate::parse_teia_source`] admits via its
    /// [`caixa_ast::NodeKind::Symbol`] projection at
    /// `manifest.rs`:`kwarg_symbol("tipo")`), and every downstream
    /// consumer that reads the identity keys off this scalar (the
    /// `caixa-arch` invariant checker's per-instance `(tipo, nome)`
    /// dedup key + `Violation::instance_tipo` diagnostic carrier + per-
    /// resource security-group / cidr-block gate, the `caixa-pangea`
    /// `InstanceToHcl` per-resource `<provider>_<kind>` Terraform-JSON
    /// type-name mint, and the substrate primitive's own [`Self::to_hcl`]
    /// per-instance HCL block-header emit path).
    ///
    /// Prior to this lift the `.tipo` field was accessed inline at ten
    /// caixa-monorepo sites — the `caixa-arch::invariants` per-invariant
    /// `unique-resource-names` / `no-unresolved-refs` / `no-public-
    /// ingress-without-tags` / `cidr-block-looks-valid` per-instance
    /// dedup-key + Violation-envelope + security-group-substring gate
    /// cascade (nine sites), and the `caixa-pangea::manifest_bridge`
    /// `InstanceToHcl::mutate` per-instance `<provider>_<kind>`
    /// Terraform-JSON type-name normalization (one site) — each expressed
    /// no compile-time link back to the typed slot. A future extension of
    /// the `:tipo` axis to a richer author surface — a per-provider
    /// alias table the substrate rewrites through the (future) `iac-
    /// forge` schema-resolution pass, a promotion of the `<provider>/
    /// <kind>` plain byte-string to a richer scoped-provider-identifier
    /// newtype once cross-provider federation lands, a per-tenant type
    /// remap the future M4 `mesh.pleme.io/v1alpha1/Aplicacao` CR
    /// materializer resolves per-CR — would have had to be threaded
    /// through every open-coded copy in lockstep or the invariant
    /// checker's `unique-resource-names` dedup key and the Terraform-
    /// JSON emit path's `tf_type` mint would silently disagree on which
    /// provider a given `TeiaInstance` resolves to. Lifting the
    /// resolution rule to a typed method on the substrate primitive
    /// means every downstream consumer reaches for exactly one typed
    /// dispatch — the resolver's accept-set migrates as a unit on any
    /// future axis addition.
    ///
    /// First `&str`-return accessor on the outer `TeiaInstance` — opens
    /// the outer-`TeiaInstance` scalar projection pattern the sibling
    /// `:nome` future lift folds on. Named `tipo()` to match the
    /// storage field's name — the accessor's identity name maps onto
    /// the author-declared `(defteia :tipo …)` vocabulary the slot's
    /// docstring already carries. Same "one typed dispatch on the
    /// substrate primitive, thin projections at each consumer"
    /// discipline as the sibling [`caixa_core::Dep::nome`] (eba2cde),
    /// per-`:membros` [`caixa_core::aplicacao::Membro::nome`] (4a32abf),
    /// and outer [`caixa_core::Caixa::nome`] (e6b7d97 and its
    /// convergence family — most recent 3219a42 / c9be435 / 41ab9a3 /
    /// 61d3429) named-caixa-referencing accessors, extended onto the
    /// substrate's IaC-side `(defteia …)` per-instance provider-
    /// identity axis.
    #[must_use]
    pub fn tipo(&self) -> &str {
        self.tipo.as_str()
    }

    /// Substrate-canonical per-`(defteia …)` `:nome` per-resource instance-
    /// identity scalar accessor every downstream IaC-facing consumer of the
    /// `TeiaInstance` primitive keys off — returns the author-declared
    /// `:nome` byte-string verbatim as a `&str`, borrowed from the typed
    /// slot's own [`String`] storage.
    ///
    /// The `:nome` slot carries the per-`(defteia …)` `main` / `primary` /
    /// `worker-a` shape (the shape [`crate::parse_teia_source`] admits via
    /// its [`caixa_ast::NodeKind::Symbol`] projection at
    /// `manifest.rs`:`kwarg_symbol("nome")`), and every downstream consumer
    /// that reads the identity keys off this scalar (the `caixa-arch`
    /// invariant checker's per-instance `(tipo, nome)` dedup key + declared-
    /// set insert + `Violation::instance_nome` diagnostic carrier over all
    /// four built-in invariants, the `caixa-pangea` `InstanceToHcl` per-
    /// resource Terraform-JSON `resource "<tf_type>" "<name>"` block-name
    /// carry, and the substrate primitive's own [`Self::to_hcl`] per-
    /// instance HCL block-header emit path).
    ///
    /// Prior to this lift the `.nome` field was accessed inline at nine
    /// caixa-monorepo sites — the substrate primitive's own [`Self::to_hcl`]
    /// block-header `resource "<tf>" "<nome>"` emit (one site), the
    /// `caixa-arch::invariants` per-invariant `unique-resource-names` dedup
    /// key + `Violation::instance_nome` diagnostic carrier + `{}` Display
    /// interpolation, `no-unresolved-refs` declared-set insert + per-`Ref`
    /// refusal `Violation::instance_nome` carrier, `no-public-ingress-
    /// without-tags` `Violation::instance_nome` carrier, `cidr-block-looks-
    /// valid` `Violation::instance_nome` carrier (seven sites), and the
    /// `caixa-pangea::manifest_bridge` `InstanceToHcl::mutate` per-instance
    /// Terraform-JSON `<name>` normalization (one site) — each expressed no
    /// compile-time link back to the typed slot. A future extension of the
    /// `:nome` axis to a richer author surface — a promotion of the plain
    /// byte-string to a richer DNS-1123-label / provider-scoped-identifier
    /// newtype once the substrate's IaC-side identity discipline converges
    /// with the CAIXA-SDLC-side [`caixa_core::Caixa::nome`] shape, a per-
    /// tenant `<name>` rewrite the future M4 `mesh.pleme.io/v1alpha1/
    /// Aplicacao` CR materializer resolves per-CR, or a per-provider
    /// `<name>` sanitizer the future `iac-forge` schema-resolution pass
    /// routes through — would have had to be threaded through every open-
    /// coded copy in lockstep, or the `unique-resource-names` dedup key and
    /// the `caixa-pangea` `resource "<tf>" "<name>"` Terraform-JSON emit
    /// path would silently disagree on which `TeiaInstance` a given
    /// per-resource declared-name resolves to; the invariant checker's
    /// `(tipo, nome)` uniqueness set treating the identity as `"main"`
    /// while the Terraform-JSON emit path treated it as `"tenant-a-main"`
    /// would silently split the build-time refusal from the runtime plan-
    /// and-apply pipeline, one gate disagreeing with the artifact the
    /// substrate's teia → pangea pipeline actually materializes. Lifting
    /// the resolution rule to a typed method on the substrate primitive
    /// means every downstream consumer reaches for exactly one typed
    /// dispatch — the resolver's accept-set migrates as a unit on any
    /// future axis addition.
    ///
    /// Second `&str`-return accessor on the outer `TeiaInstance` — folds on
    /// the outer-`TeiaInstance` scalar projection pattern the sibling
    /// [`Self::tipo`] accessor (e58baa7) opened, and closes the two
    /// author-declared universal-axis identity-scalar slots on the
    /// substrate's IaC-side per-`(defteia …)` primitive. Named `nome()` to
    /// match the storage field's name — the accessor's identity name maps
    /// onto the author-declared `(defteia :nome …)` vocabulary the slot's
    /// docstring already carries. Same "one typed dispatch on the substrate
    /// primitive, thin projections at each consumer" discipline as the
    /// sibling [`Self::tipo`] (e58baa7), [`caixa_core::Dep::nome`]
    /// (eba2cde), per-`:membros` [`caixa_core::aplicacao::Membro::nome`]
    /// (4a32abf), and outer [`caixa_core::Caixa::nome`] (e6b7d97 and its
    /// convergence family — most recent 3219a42 / c9be435 / 41ab9a3 /
    /// 61d3429) named-caixa-referencing accessors, extended onto the
    /// substrate's IaC-side `(defteia …)` per-instance instance-identity
    /// axis.
    #[must_use]
    pub fn nome(&self) -> &str {
        self.nome.as_str()
    }

    /// Append an attribute — fluent builder.
    #[must_use]
    pub fn with_attr(mut self, key: impl Into<String>, value: TeiaValue) -> Self {
        self.atributos.insert(key.into(), value);
        self
    }

    /// Terraform-style `resource "aws_vpc" "main" { … }` rendering.
    #[must_use]
    pub fn to_hcl(&self) -> String {
        // Route the internal `<provider>_<kind>` type-name mint through
        // the lifted [`Self::tipo`] scalar accessor rather than the raw
        // `self.tipo` field access — the substrate primitive's own HCL
        // block-header emit path now keys off the canonical raw-slot
        // surface every downstream [`caixa_arch`] / [`caixa_pangea`]
        // per-`TeiaInstance` `:tipo` consumer routes through, so any
        // future rebrand on the typed slot's raw-slot reader lands at
        // exactly one place.
        let tf_tipo = self.tipo().replace('/', "_");
        // Route the internal `<name>` HCL block-header carry through the
        // lifted [`Self::nome`] scalar accessor — same "one typed dispatch
        // on the substrate primitive, thin projections at each consumer"
        // discipline the sibling [`Self::tipo`] read at the `<tf_tipo>`
        // block-header carry already follows.
        let mut out = format!("resource \"{tf_tipo}\" \"{}\" {{", self.nome());
        out.push('\n');
        for (k, v) in &self.atributos {
            let _ = writeln!(out, "  {k} = {}", v.to_hcl_string());
        }
        out.push_str("}\n");
        out
    }

    /// Quick validation against an iac-forge schema: every required attribute
    /// must be present in this instance.
    #[must_use]
    pub fn missing_required(&self, schema: &iac_forge::ir::IacResource) -> Vec<String> {
        use iac_forge::ir::HasAttributes;
        schema
            .required_attribute_names()
            .into_iter()
            .filter(|name| !self.atributos.contains_key(&(*name).to_string()))
            .map(ToString::to_string)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::TeiaRef;
    use crate::value::TeiaValue;

    #[test]
    fn tipo_returns_declared_tipo_verbatim() {
        // The accessor is a projection, not a gate. Every author-
        // declared `:tipo` byte-string — the canonical `aws/vpc` shape,
        // a hyphenated `google/compute-instance` shape, a snake_cased
        // `aws_iam_role` shape (already-normalized), a namespaced
        // `akeyless/static-secret`, an empty `""` sentinel — round-
        // trips as-is through `TeiaInstance::tipo()`. Pins the
        // "verbatim projection" contract every downstream consumer
        // (`caixa-arch::invariants` dedup key, `caixa-pangea::manifest_
        // bridge` `<provider>_<kind>` mint, `TeiaInstance::to_hcl`
        // block-header emit) depends on.
        for fixture in [
            "aws/vpc",
            "akeyless/static-secret",
            "google/compute-instance",
            "aws_iam_role",
            "",
        ] {
            let inst = TeiaInstance::new(fixture, "main");
            assert_eq!(
                inst.tipo(),
                fixture,
                "TeiaInstance::tipo must return the author-declared :tipo \
                 byte-string verbatim (fixture: {fixture:?})",
            );
        }
    }

    #[test]
    fn tipo_is_by_borrow_pointer_identity() {
        // Zero-copy pin — `inst.tipo()` must borrow from the typed
        // slot's own [`String`] storage, not clone into a fresh buffer.
        // Fails at build time if a future rewrite regresses to an
        // owned-buffer shape (`self.tipo.clone()` in the body would
        // type-check but silently allocate on every call — the pointer
        // identity check catches it).
        let inst = TeiaInstance::new("aws/vpc", "main");
        let via_accessor: &str = inst.tipo();
        assert_eq!(
            via_accessor.as_ptr(),
            inst.tipo.as_ptr(),
            "TeiaInstance::tipo must borrow from the .tipo String's \
             backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            inst.tipo.len(),
            "TeiaInstance::tipo and .tipo.as_str() must byte-equal in \
             length (same slice)",
        );
    }

    #[test]
    fn tipo_agrees_with_parsed_source() {
        // Composition pin: the accessor projects through the parser's
        // own `kwarg_symbol("tipo")` capture — any future rebrand of
        // the parser-side `:tipo` reader lands at exactly one place
        // and both `parse_teia_source(…).instances[0].tipo()` and the
        // raw slot byte-equal each other.
        let src = r#"(defteia :tipo aws/vpc :nome main
                     :atributos (:cidr-block "10.0.0.0/16"))"#;
        let m = crate::parse_teia_source(src).unwrap();
        let inst = &m.instances[0];
        assert_eq!(inst.tipo(), "aws/vpc");
        assert_eq!(inst.tipo(), inst.tipo.as_str());
    }

    #[test]
    fn to_hcl_reader_routes_through_tipo_accessor() {
        // Coherence pin: the substrate primitive's own `to_hcl` reader
        // and every downstream `caixa-arch` / `caixa-pangea` per-`:tipo`
        // consumer share the same accessor. Regresses if a future
        // detour re-inlines the raw `self.tipo` field access at the
        // HCL block-header emit path.
        let inst = TeiaInstance::new("aws/vpc", "main")
            .with_attr("cidr_block", TeiaValue::Str("10.0.0.0/16".into()));
        let hcl = inst.to_hcl();
        assert!(
            hcl.contains("resource \"aws_vpc\" \"main\""),
            "to_hcl must mint <provider>_<kind> through the .tipo() accessor",
        );
    }

    #[test]
    fn nome_returns_declared_nome_verbatim() {
        // The accessor is a projection, not a gate. Every author-declared
        // `:nome` byte-string — the canonical `main` shape, a hyphenated
        // `worker-a` shape, a namespaced-ish `primary-01` shape, a
        // digit-prefixed `v2-worker` shape, an empty `""` sentinel —
        // round-trips as-is through `TeiaInstance::nome()`. Pins the
        // "verbatim projection" contract every downstream consumer
        // (`caixa-arch::invariants` `(tipo, nome)` dedup key, `caixa-pangea
        // ::manifest_bridge` `<name>` normalization, `TeiaInstance::to_hcl`
        // block-header emit) depends on.
        for fixture in ["main", "worker-a", "primary-01", "v2-worker", ""] {
            let inst = TeiaInstance::new("aws/vpc", fixture);
            assert_eq!(
                inst.nome(),
                fixture,
                "TeiaInstance::nome must return the author-declared :nome \
                 byte-string verbatim (fixture: {fixture:?})",
            );
        }
    }

    #[test]
    fn nome_is_by_borrow_pointer_identity() {
        // Zero-copy pin — `inst.nome()` must borrow from the typed slot's
        // own [`String`] storage, not clone into a fresh buffer. Fails at
        // build time if a future rewrite regresses to an owned-buffer
        // shape (`self.nome.clone()` in the body would type-check but
        // silently allocate on every call — the pointer identity check
        // catches it).
        let inst = TeiaInstance::new("aws/vpc", "main");
        let via_accessor: &str = inst.nome();
        assert_eq!(
            via_accessor.as_ptr(),
            inst.nome.as_ptr(),
            "TeiaInstance::nome must borrow from the .nome String's \
             backing storage (zero-copy projection)",
        );
        assert_eq!(
            via_accessor.len(),
            inst.nome.len(),
            "TeiaInstance::nome and .nome.as_str() must byte-equal in \
             length (same slice)",
        );
    }

    #[test]
    fn nome_agrees_with_parsed_source() {
        // Composition pin: the accessor projects through the parser's own
        // `kwarg_symbol("nome")` capture — any future rebrand of the
        // parser-side `:nome` reader lands at exactly one place and both
        // `parse_teia_source(…).instances[0].nome()` and the raw slot
        // byte-equal each other.
        let src = r#"(defteia :tipo aws/vpc :nome main
                     :atributos (:cidr-block "10.0.0.0/16"))"#;
        let m = crate::parse_teia_source(src).unwrap();
        let inst = &m.instances[0];
        assert_eq!(inst.nome(), "main");
        assert_eq!(inst.nome(), inst.nome.as_str());
    }

    #[test]
    fn to_hcl_reader_routes_through_nome_accessor() {
        // Coherence pin: the substrate primitive's own `to_hcl` reader and
        // every downstream `caixa-arch` / `caixa-pangea` per-`:nome`
        // consumer share the same accessor. Regresses if a future detour
        // re-inlines the raw `self.nome` field access at the HCL block-
        // header emit path.
        let inst = TeiaInstance::new("aws/vpc", "primary")
            .with_attr("cidr_block", TeiaValue::Str("10.0.0.0/16".into()));
        let hcl = inst.to_hcl();
        assert!(
            hcl.contains("resource \"aws_vpc\" \"primary\""),
            "to_hcl must carry <name> through the .nome() accessor",
        );
    }

    #[test]
    fn hcl_rendering() {
        let vpc = TeiaInstance::new("aws/vpc", "main")
            .with_attr("cidr_block", TeiaValue::Str("10.0.0.0/16".into()));
        let out = vpc.to_hcl();
        assert!(out.contains("resource \"aws_vpc\" \"main\""));
        assert!(out.contains(r#"cidr_block = "10.0.0.0/16""#));
    }

    #[test]
    fn ref_rendering() {
        let r = TeiaRef::new("aws/vpc", "main").atributo("id");
        let v = TeiaValue::Ref(r);
        assert_eq!(v.to_hcl_string(), "${aws_vpc.main.id}");
    }
}
