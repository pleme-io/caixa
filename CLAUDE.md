# Caixa

> **★★★ CSE / Knowable Construction.** This repo operates under **Constructive Substrate Engineering** — canonical specification at [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md). The Compounding Directive is in the org-level pleme-io/CLAUDE.md ★★★ section. Companion theory: [`CAIXA-SDLC.md`](https://github.com/pleme-io/theory/blob/main/CAIXA-SDLC.md) (the author-to-live playbook), [`MESH-COMPOSITION.md`](https://github.com/pleme-io/theory/blob/main/MESH-COMPOSITION.md) (typed Aplicacao), [`INSPIRATIONS.md`](https://github.com/pleme-io/theory/blob/main/INSPIRATIONS.md) (Erlang/OTP, Lunatic, Unison, Pony, Akka — what to absorb), [`RUNTIME-PATTERNS.md`](https://github.com/pleme-io/theory/blob/main/RUNTIME-PATTERNS.md) (~30 runtime patterns + status), [`ABSORPTION-ROADMAP.md`](https://github.com/pleme-io/theory/blob/main/ABSORPTION-ROADMAP.md) (M2-M5 deliverables).

caixa — the typed package + lifecycle primitive for the pleme-io
substrate. Author surface is `(defcaixa …)`; substrate-side renderers
generate every cluster artifact mechanically. The companion CLI is
[`feira`](caixa-feira/), the operator runs in the cluster as
[`caixa-operator`](caixa-operator/), and the typed renderers
([`caixa-helm`](caixa-helm/), [`caixa-flux`](caixa-flux/),
[`caixa-mesh`](caixa-mesh/)) emit Helm charts, FluxCD bundles, and
mesh primitives respectively.

## ★★ The five typed kinds

`:kind <Kind>` selects the runtime contract. Every caixa is exactly one of:

| kind | runs | author surface | renders to |
|---|---|---|---|
| `Biblioteca` | nothing — exports forms | `lib/<nome>.lisp` | (consumer caixas via `(importar :caixa …)`) |
| `Binario` | locally (CLI) | `exe/<nome>.lisp` | nix-built binary |
| `Servico` | as a wasm component | `servicos/<nome>.computeunit.yaml` + source | OCI image + ComputeUnit CR + `lareira-<nome>` Helm chart |
| `Supervisor` | nothing — supervises children | `:children` + `:estrategia` | hierarchical reconciliation |
| `Aplicacao` | nothing — composes Servicos | `:membros` + `:contratos` + `:politicas` + `:placement` + `:entrada` | programs.yaml fan-out + Cilium NetworkPolicies + Gateway/HTTPRoute |

## ★ M2 typed slots (every Servico can declare)

| slot | source pattern | what it's for |
|---|---|---|
| `:limits` | Lunatic per-process sandboxing | memory cap, fuel cap, wall-clock cap, soft cgroup CPU |
| `:behavior` | OTP `gen_server` callbacks | `:on-init`, `:on-call`, `:on-cast`, `:on-info`, `:on-state-change`, `:on-terminate` paths |
| `:upgrade-from` | OTP appup | typed migration instructions per prior `:versao` (`LoadModule | StateChange | SoftPurge | Purge | Restart`) |
| `:estrategia` + `:children` (Supervisor only) | OTP supervisor strategies | `OneForOne | OneForAll | RestForOne | SimpleOneForOne` over typed children with `Permanent | Temporary | Transient` restart policies |

## ★ M3 typed mesh slots (every Aplicacao must declare)

| slot | what it carries |
|---|---|
| `:membros` | the Servicos that make up this app (caixa-name + version constraint per entry) |
| `:contratos` | WIT-typed inter-Servico edges (`:de` → `:para`, with `:wit "wasi:http/proxy" | "nats:pub-sub" | "wasi:keyvalue/store"` and the appropriate `:endpoint`/`:subject`/`:slot`) |
| `:politicas` | mesh-level: `:timeout`, `:retries`, `:circuit-breaker (:max-failures :window)`, `:mtls-required`, `:rate-limit` (`"100/s"` form) |
| `:placement` | `:estrategia SingleNode | Replicated | Sharded` + `:clusters` list + optional `:shard-key`/`:affinity` |
| `:entrada` | external gateway: `:host`, `:para`, `:paths`, `:port` |

The build refuses Aplicacaos whose `:contratos` reference unknown
members, whose `:placement Sharded` lacks `:shard-key`, or whose
`:placement Replicated/SingleNode` lacks `:clusters`. Every guarantee
in [`theory/MESH-COMPOSITION.md`](https://github.com/pleme-io/theory/blob/main/MESH-COMPOSITION.md) §III.3 is type-system-enforced.

## ★ The feira CLI

| verb | scope |
|---|---|
| `feira init <nome>` | scaffold a new caixa |
| `feira add <dep>` | append a dep to caixa.lisp |
| `feira lint` | parse + Nord-themed lint report |
| `feira fmt` | canonical formatter |
| `feira build` | layout invariants + parse every declared lib |
| `feira lock` / `resolve` | git-clone deps + write lacre.lisp (BLAKE3 closure) |
| `feira chart` | render `lareira-<nome>` Helm chart for a Servico |
| `feira deploy --cluster <name>` | upsert Servico into the cluster's fleet manifest |
| `feira app graph` | print typed Aplicacao graph |
| `feira app deploy --cluster <name>` | render whole-Aplicacao multi-doc YAML + write to k8s repo |
| `feira nix` | emit flake.nix |
| `feira publish` | Zig-style git-tag publish |
| `feira tofu` | (for `:kind Biblioteca` infra caixas) end-to-end tofu plan/apply |

## ★ Workspace crates

| crate | role |
|---|---|
| `caixa-core` | typed manifest types (Caixa, AplicacaoSpec, SupervisorSpec, LimitsSpec, BehaviorSpec, UpgradeFromEntry, etc.) + layout invariants. The substrate's load-bearing type system. |
| `caixa-feira` | the `feira` CLI |
| `caixa-helm` | renderer: Caixa Servico → `lareira-<nome>` Helm chart (Chart.yaml + values.yaml depending on `pleme-computeunit` library chart) |
| `caixa-flux` | renderer: Caixa Servico → programs.yaml entry + standalone GitRepository/HelmRelease/Kustomization bundle |
| `caixa-mesh` | renderer: Caixa Aplicacao → programs.yaml fan-out + CiliumNetworkPolicies + Gateway/HTTPRoute |
| `caixa-resolver` | typed git-source resolution; emits `lacre.lisp` BLAKE3 closure |
| `caixa-operator` | K8s operator watching `Caixa`/`Lacre`/`CaixaBuild` CRs + reconciling builds |
| `caixa-crd` | CRD types for the operator |
| `caixa-arch` / `caixa-teia` / `caixa-teia-forge` / `caixa-pangea` / `caixa-flake` | typed renderers for IaC + Nix flakes |
| `caixa-fmt` / `caixa-lint` / `caixa-lsp` / `caixa-ast` | authoring tooling |
| `caixa-lacre` | typed Lacre (BLAKE3 closure) types |
| `caixa-provedor` | provider abstraction |
| `caixa-theme` | Nord palette for Lisp output |
| `operator-chart/` | Helm chart that deploys `caixa-operator` (CRDs + RBAC) |
| `operator-flux/` | FluxCD manifests for in-cluster `caixa-operator` reconciliation |

## ★ Reusable CI workflows (in pleme-io/substrate)

Every caixa repo's `.github/workflows/release.yml` is **5–10 lines**
pointing at one of:

- `pleme-io/substrate/.github/workflows/caixa-publish.yml@main` — Servico (Rust→wasm OCI + git tag)
- `pleme-io/substrate/.github/workflows/caixa-publish-tlisp.yml@main` — pure-Lisp Servico (git tag only)
- `pleme-io/substrate/.github/workflows/caixa-validate.yml@main` — non-publishing gate (cse-lint + feira lint)
- `pleme-io/substrate/.github/workflows/caixa-forge.yml@main` — forge-gen integration (auto-PR on drift)

Every CI run gates against the 6 CSE invariants via `nix run
github:pleme-io/cse-lint -- repo . --strict`. The `caixa-naivete`
checker (the 6th invariant) flags any pleme-io repo lacking a
`caixa.lisp` at the root.

## ★ Canonical examples

- [`hello-rio`](https://github.com/pleme-io/hello-rio) — Rust→wasm Servico (canonical Rust path)
- [`programs/hello-world`](https://github.com/pleme-io/programs/tree/main/hello-world) — tatara-lisp Servico (canonical Lisp path)
- [`examples/checkout-aplicacao/`](./examples/checkout-aplicacao) — typed Aplicacao composing 4 Servicos with HTTP + NATS contratos

## ★ Skill

`caixa-author` (in [`pleme-io/blackmatter-pleme/skills/caixa-author`](https://github.com/pleme-io/blackmatter-pleme/tree/main/skills/caixa-author)) — when authoring a new caixa or migrating an existing repo, invoke this skill first. Companion: `tatara-lisp-program` for the Lisp-source program internals (snapshot/receive-state shapes, runtime cookbook patterns).
