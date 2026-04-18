//! Built-in invariants — the ones a reasonable infra caixa should never break.
//!
//! Each invariant is a pure `fn(&TeiaManifest) -> Vec<Violation>`.  Custom
//! policies are phase-2 work (iac-forge's `policy::Policy` is the extension
//! point — we don't depend on its full tree here to stay light).

use caixa_teia::{TeiaInstance, TeiaManifest, TeiaValue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvariantKind {
    /// A hard safety property — refuse to emit HCL when violated.
    Safety,
    /// A compliance property — report, don't block by default.
    Compliance,
    /// A best-practice hint — never blocks.
    Hint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub invariant_id: String,
    pub kind: InvariantKind,
    pub instance_tipo: String,
    pub instance_nome: String,
    pub message: String,
}

#[derive(Clone)]
pub struct Invariant {
    pub id: &'static str,
    pub kind: InvariantKind,
    pub description: &'static str,
    pub check: fn(&TeiaManifest) -> Vec<Violation>,
}

#[must_use]
pub fn builtin_invariants() -> Vec<Invariant> {
    vec![
        Invariant {
            id: "unique-resource-names",
            kind: InvariantKind::Safety,
            description: "no two instances share (tipo, nome) — Terraform would reject it",
            check: unique_resource_names,
        },
        Invariant {
            id: "no-unresolved-refs",
            kind: InvariantKind::Safety,
            description: "every (ref tipo nome attr) points at an instance that exists",
            check: no_unresolved_refs,
        },
        Invariant {
            id: "no-public-ingress-without-tags",
            kind: InvariantKind::Compliance,
            description: "resources exposed to 0.0.0.0/0 must carry an owner/team tag",
            check: no_public_ingress_without_tags,
        },
        Invariant {
            id: "cidr-block-looks-valid",
            kind: InvariantKind::Hint,
            description: ":cidr-block values should look like IPv4/CIDR notation",
            check: cidr_block_format_hint,
        },
    ]
}

fn unique_resource_names(m: &TeiaManifest) -> Vec<Violation> {
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for inst in &m.instances {
        let key = (inst.tipo.clone(), inst.nome.clone());
        if !seen.insert(key.clone()) {
            out.push(Violation {
                invariant_id: "unique-resource-names".into(),
                kind: InvariantKind::Safety,
                instance_tipo: inst.tipo.clone(),
                instance_nome: inst.nome.clone(),
                message: format!(
                    "duplicate instance {} / {} — Terraform resource names must be unique per type",
                    inst.tipo, inst.nome
                ),
            });
        }
    }
    out
}

fn no_unresolved_refs(m: &TeiaManifest) -> Vec<Violation> {
    let mut declared: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for inst in &m.instances {
        declared.insert((inst.tipo.clone(), inst.nome.clone()));
    }
    let mut out = Vec::new();
    for inst in &m.instances {
        for v in inst.atributos.values() {
            collect_ref_violations(inst, v, &declared, &mut out);
        }
    }
    out
}

fn collect_ref_violations(
    inst: &TeiaInstance,
    v: &TeiaValue,
    declared: &std::collections::HashSet<(String, String)>,
    out: &mut Vec<Violation>,
) {
    match v {
        TeiaValue::Ref(r) => {
            if !declared.contains(&(r.tipo.clone(), r.nome.clone())) {
                out.push(Violation {
                    invariant_id: "no-unresolved-refs".into(),
                    kind: InvariantKind::Safety,
                    instance_tipo: inst.tipo.clone(),
                    instance_nome: inst.nome.clone(),
                    message: format!(
                        "(ref {} {} {}) targets an undeclared instance",
                        r.tipo, r.nome, r.atributo
                    ),
                });
            }
        }
        TeiaValue::List(items) => items
            .iter()
            .for_each(|i| collect_ref_violations(inst, i, declared, out)),
        TeiaValue::Object(map) => map
            .values()
            .for_each(|i| collect_ref_violations(inst, i, declared, out)),
        _ => {}
    }
}

fn no_public_ingress_without_tags(m: &TeiaManifest) -> Vec<Violation> {
    let mut out = Vec::new();
    for inst in &m.instances {
        let sg_like = inst.tipo.contains("security-group") || inst.tipo.contains("security_group");
        let has_public_cidr = flatten_strings(&inst.atributos)
            .iter()
            .any(|s| s.contains("0.0.0.0/0"));
        let has_owner_tag = inst
            .atributos
            .get("tags")
            .and_then(|v| match v {
                TeiaValue::Object(m) => Some(m),
                _ => None,
            })
            .is_some_and(|m| {
                m.keys().any(|k| {
                    k.eq_ignore_ascii_case("owner")
                        || k.eq_ignore_ascii_case("team")
                        || k.eq_ignore_ascii_case("dono")
                })
            });
        if sg_like && has_public_cidr && !has_owner_tag {
            out.push(Violation {
                invariant_id: "no-public-ingress-without-tags".into(),
                kind: InvariantKind::Compliance,
                instance_tipo: inst.tipo.clone(),
                instance_nome: inst.nome.clone(),
                message: "public-ingress security group needs :owner or :team tag".into(),
            });
        }
    }
    out
}

fn cidr_block_format_hint(m: &TeiaManifest) -> Vec<Violation> {
    let mut out = Vec::new();
    for inst in &m.instances {
        if let Some(TeiaValue::Str(s)) = inst.atributos.get("cidr-block") {
            if !looks_like_cidr(s) {
                out.push(Violation {
                    invariant_id: "cidr-block-looks-valid".into(),
                    kind: InvariantKind::Hint,
                    instance_tipo: inst.tipo.clone(),
                    instance_nome: inst.nome.clone(),
                    message: format!(":cidr-block {s:?} does not look like IPv4/CIDR"),
                });
            }
        }
    }
    out
}

fn looks_like_cidr(s: &str) -> bool {
    let Some((ip, mask)) = s.split_once('/') else {
        return false;
    };
    if mask.parse::<u8>().map_or(true, |m| m > 32) {
        return false;
    }
    let parts: Vec<&str> = ip.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
}

fn flatten_strings(map: &std::collections::BTreeMap<String, TeiaValue>) -> Vec<String> {
    let mut out = Vec::new();
    for v in map.values() {
        collect_strings(v, &mut out);
    }
    out
}

fn collect_strings(v: &TeiaValue, out: &mut Vec<String>) {
    match v {
        TeiaValue::Str(s) => out.push(s.clone()),
        TeiaValue::List(items) => items.iter().for_each(|i| collect_strings(i, out)),
        TeiaValue::Object(m) => m.values().for_each(|i| collect_strings(i, out)),
        _ => {}
    }
}
