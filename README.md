# caixa

Typed package + lifecycle primitive for the [pleme-io](https://github.com/pleme-io)
substrate. Author surface is `(defcaixa …)`; substrate-side renderers
generate every cluster artifact mechanically. Cargo-shaped semantics
+ Erlang/OTP-shaped lifecycles + Lunatic-shaped sandboxing + Unison-shaped
content addressing — all unified under one typed authoring surface.

```lisp
(defcaixa
  :nome      "hello-rio"
  :versao    "0.1.0"
  :kind      Servico
  :limits    ((:memory "64MiB") (:fuel 1000000) (:wall-clock "30s"))
  :behavior  ((:on-call "lib/handlers.lisp"))
  :servicos  ("servicos/hello-rio.computeunit.yaml"))
```

That's the whole authoring surface for a service. CI builds the
wasm component, publishes the OCI image, tags the git ref. `feira deploy
--cluster rio --apply` writes the cluster manifest, FluxCD reconciles,
the operator deploys the pod. All typed; all generated.

## What it is

caixa is a **typed package primitive**: every author-authored unit of
compute on the substrate is a caixa, identified by a hash of its
fully-resolved AST (lacre BLAKE3 closure), authored as a single
tatara-lisp `(defcaixa …)` form, and rendered to whatever cluster
shape the `:kind` selects:

- **`Biblioteca`** — a tatara-lisp library that other caixas
  `(importar :caixa "name")`. Rendered to (nothing — it's source).
- **`Binario`** — a locally-invokable CLI. Rendered to a nix-built binary.
- **`Servico`** — a long-running wasm component on the cluster.
  Rendered to: OCI image, K8s `ComputeUnit` CR, `lareira-<nome>`
  Helm chart.
- **`Supervisor`** — a typed parent of N child caixas with an
  Erlang/OTP-shaped restart strategy (`OneForOne | OneForAll |
  RestForOne | SimpleOneForOne`). Rendered to: hierarchical
  reconciliation by the wasm-operator.
- **`Aplicacao`** — a typed mesh of Servicos with WIT-typed inter-
  Servico contracts, mesh policies (timeouts, retries, mTLS, rate
  limits, circuit breakers), placement strategy (single-node /
  replicated / sharded), and external entry. Rendered to: programs.yaml
  fan-out + Cilium NetworkPolicies + K8s Gateway/HTTPRoute.

The runtime substrate (the wasm-engine + wasm-operator + lareira-tatara-stack
on Kubernetes, with Cilium for identity-based mesh authorization)
turns the rendered artifacts into running pods. All of it composes
out of one typed authoring surface.

## Why

Containers couple code with state, capabilities, identity, and
deployment shape. caixa decouples all four: code is content-addressed
(lacre BLAKE3), state is explicit (PVC / event-sourced log / KV),
capabilities are typed tokens (`:capabilities` slot), identity is
the lacre closure root (becomes Cilium SPIFFE ID directly). Every
axis is named, typed, and independently composable.

The result: you can change one without disturbing the others. Hot
upgrades become typed migrations (`:upgrade-from`), cross-cluster
migration becomes a placement strategy change (`:placement
:replicated → :sharded`), credential rotation requires no restart
(`:capabilities` updated, downward API propagates). The pattern
catalog in [`theory/RUNTIME-PATTERNS.md`](https://github.com/pleme-io/theory/blob/main/RUNTIME-PATTERNS.md)
lists ~30 concrete patterns the substrate enables; the prior-art
catalog in [`theory/INSPIRATIONS.md`](https://github.com/pleme-io/theory/blob/main/INSPIRATIONS.md)
maps each one to the tradition (Erlang/OTP, Lunatic, Common Lisp,
Smalltalk, Unison, Pony, Akka, Orleans, CRIU, eBPF) it absorbs.

## Quickstart

```bash
# Scaffold a new Servico:
nix run github:pleme-io/caixa#feira -- init my-service --path .
$EDITOR caixa.lisp                    # set :kind, :descricao, :limits, etc.

# Author the K8s ComputeUnit runtime contract:
mkdir -p servicos
$EDITOR servicos/my-service.computeunit.yaml

# Wire CI (5 lines):
mkdir -p .github/workflows && cat > .github/workflows/release.yml <<'EOF'
name: release
on: { push: { branches: [main] }, workflow_dispatch: {} }
jobs:
  release:
    uses: pleme-io/substrate/.github/workflows/caixa-publish.yml@main
    secrets: inherit
    permissions: { contents: write, packages: write }
EOF

# Verify locally:
nix run github:pleme-io/cse-lint -- repo . --strict
nix run github:pleme-io/caixa#feira -- lint
nix run github:pleme-io/caixa#feira -- chart    # render lareira-my-service/

# Push:
git add caixa.lisp servicos/ .github/workflows/release.yml
git commit -m "init: my-service caixa Servico"
git push origin main

# CI builds the wasm component, publishes the OCI image, tags v0.1.0.
# To deploy:
nix run github:pleme-io/caixa#feira -- deploy --cluster rio --apply
```

For an Aplicacao composing N Servicos, swap `:kind Servico` for
`:kind Aplicacao` and use `feira app graph` / `feira app deploy`
instead. See [`examples/checkout-aplicacao/`](./examples/checkout-aplicacao)
for the canonical demonstration.

## Repo layout

| crate | role |
|---|---|
| [`caixa-core`](caixa-core/) | typed manifest + layout invariants — load-bearing type system |
| [`caixa-feira`](caixa-feira/) | `feira` CLI — init, lint, build, chart, deploy, app graph, app deploy, publish |
| [`caixa-helm`](caixa-helm/) | renderer: Servico → `lareira-<nome>` Helm chart |
| [`caixa-flux`](caixa-flux/) | renderer: Servico → programs.yaml entry + GitRepository/HelmRelease/Kustomization |
| [`caixa-mesh`](caixa-mesh/) | renderer: Aplicacao → programs fan-out + CiliumNetworkPolicies + Gateway/HTTPRoute |
| [`caixa-resolver`](caixa-resolver/) | git source resolution + lacre.lisp BLAKE3 closure |
| [`caixa-operator`](caixa-operator/) | K8s operator watching Caixa/Lacre/CaixaBuild CRs |
| [`caixa-crd`](caixa-crd/) | typed CRD shapes for the operator |
| [`caixa-arch`](caixa-arch/) / [`caixa-teia`](caixa-teia/) / [`caixa-teia-forge`](caixa-teia-forge/) | IaC composition layers |
| [`caixa-pangea`](caixa-pangea/) / [`caixa-flake`](caixa-flake/) | typed renderers (Pangea Ruby + Nix flakes) |
| [`caixa-fmt`](caixa-fmt/) / [`caixa-lint`](caixa-lint/) / [`caixa-lsp`](caixa-lsp/) / [`caixa-ast`](caixa-ast/) | authoring tooling |
| [`caixa-lacre`](caixa-lacre/) | typed Lacre (BLAKE3 closure) types |
| [`caixa-provedor`](caixa-provedor/) | provider abstraction layer |
| [`caixa-theme`](caixa-theme/) | Nord palette for Lisp output |
| [`operator-chart/`](operator-chart/) | Helm chart deploying caixa-operator (CRDs + RBAC) |
| [`operator-flux/`](operator-flux/) | FluxCD manifests for in-cluster caixa-operator reconciliation |
| [`examples/`](examples/) | canonical example caixas (checkout-aplicacao, pangea-tatara-akeyless) |

## Documentation

| path | what's in it |
|---|---|
| [`theory/CAIXA-SDLC.md`](https://github.com/pleme-io/theory/blob/main/CAIXA-SDLC.md) | full author-to-live SDLC playbook (8 sections) |
| [`theory/META-FRAMEWORK.md`](https://github.com/pleme-io/theory/blob/main/META-FRAMEWORK.md) | the four-layer compute hierarchy + decision tree |
| [`theory/MESH-COMPOSITION.md`](https://github.com/pleme-io/theory/blob/main/MESH-COMPOSITION.md) | typed Aplicacao + Cilium-style identity-based mesh |
| [`theory/INSPIRATIONS.md`](https://github.com/pleme-io/theory/blob/main/INSPIRATIONS.md) | Erlang/OTP, Lunatic, Unison, Common Lisp, Smalltalk, Pony, Akka, Orleans, CRIU, eBPF — what to absorb verbatim, adapt, deliberately reject |
| [`theory/RUNTIME-PATTERNS.md`](https://github.com/pleme-io/theory/blob/main/RUNTIME-PATTERNS.md) | ~30 patterns (live deploy, migration, edge update, multi-tenant, observability, …) with status flags |
| [`theory/ABSORPTION-ROADMAP.md`](https://github.com/pleme-io/theory/blob/main/ABSORPTION-ROADMAP.md) | concrete M2-M5 deliverables: files, LoC, tests, dependencies |

## Skills

- `caixa-author` (in [`blackmatter-pleme`](https://github.com/pleme-io/blackmatter-pleme/tree/main/skills/caixa-author)) — invoke when scaffolding any new caixa or composing an Aplicacao
- `tatara-lisp-program` (companion) — Lisp-source program internals (snapshot/receive-state, runtime cookbook patterns)

## License

MIT — see [`LICENSE`](LICENSE).
