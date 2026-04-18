//! Reconcile loop for the `Caixa` CRD.
//!
//! Simple finite state:
//!   1. Observe a Caixa's `spec.source` (Git URL + ref).
//!   2. Clone or fetch into the operator's cache, checkout the ref.
//!   3. Read the target repo's `caixa.lisp`, parse to `caixa_core::Caixa`.
//!   4. Run `caixa_resolver::resolve_lacre` to produce the full closure.
//!   5. Create/patch the companion `Lacre` CR (name = caixa name) with the
//!      resolved entries in `.status`.
//!   6. Patch the Caixa's own status with `observedGeneration`,
//!      `resolvedRev`, `fechamentoRoot`, `ready`.
//!
//! Errors mark `Ready=False` with a human-readable reason; retries are the
//! caller-controlled default `finalizer`-style requeue via kube-runtime's
//! `Controller`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use caixa_crd::caixa_cr::{Caixa as CaixaCr, CaixaStatus, Condition};
use caixa_crd::lacre_cr::{Lacre as LacreCr, LacreEntryCr, LacreSpec, LacreStatus};
use futures::StreamExt;
use kube::{
    Client, Error as KubeError,
    api::{Api, Patch, PatchParams, PostParams, ResourceExt},
    runtime::{Controller, controller::Action},
};
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct Context {
    pub client: Client,
    pub cache_dir: std::path::PathBuf,
}

pub async fn run(client: Client, namespace: &str, oneshot: bool) -> Result<()> {
    let caixa_api: Api<CaixaCr> = if namespace.is_empty() {
        Api::all(client.clone())
    } else {
        Api::namespaced(client.clone(), namespace)
    };

    let cache_dir = std::env::temp_dir().join("caixa-operator-cache");
    std::fs::create_dir_all(&cache_dir)?;

    let ctx = Arc::new(Context {
        client: client.clone(),
        cache_dir,
    });

    let controller = Controller::new(
        caixa_api,
        kube::runtime::watcher::Config::default().any_semantic(),
    );

    let stream = Box::pin(controller.run(reconcile, on_error, ctx));
    stream
        .for_each(|res| async move {
            match res {
                Ok((obj, _)) => info!(namespace = ?obj.namespace, name = %obj.name, "reconciled"),
                Err(e) => error!(error = %e, "reconcile error"),
            }
        })
        .await;
    let _ = oneshot; // phase 1: one-shot drain will be wired once we have pin_mut helpers
    Ok(())
}

async fn reconcile(caixa: Arc<CaixaCr>, ctx: Arc<Context>) -> Result<Action, ReconcileError> {
    let ns = caixa.namespace().unwrap_or_else(|| "default".to_string());
    let name = caixa.name_any();

    info!(%ns, %name, "reconciling Caixa");

    // Parse the source-side caixa.lisp via the resolver.
    let core_caixa = caixa_crd::caixa_from_cr(&caixa);
    let cache = caixa_resolver::CacheDir::at(&ctx.cache_dir);
    let cfg = caixa_resolver::ResolverConfig::default();

    let (lacre, resolved_rev, error_msg) =
        match caixa_resolver::resolve_lacre(&core_caixa, &cfg, &cache) {
            Ok(l) => {
                let rev = l
                    .entradas
                    .first()
                    .map(|e| e.conteudo.clone())
                    .unwrap_or_default();
                (Some(l), rev, None)
            }
            Err(e) => (None, String::new(), Some(e.to_string())),
        };

    // Write companion Lacre CR if resolution succeeded.
    if let Some(lacre) = &lacre {
        let lacre_api: Api<LacreCr> = Api::namespaced(ctx.client.clone(), &ns);
        let lacre_name = format!("{name}-lacre");
        let spec = LacreSpec {
            caixa_ref: name.clone(),
            resolved_rev: resolved_rev.clone(),
        };
        let mut cr = LacreCr::new(&lacre_name, spec);
        cr.status = Some(LacreStatus {
            raiz: Some(lacre.raiz.clone()),
            entrada_count: Some(i32::try_from(lacre.entradas.len()).unwrap_or(0)),
            entradas: lacre
                .entradas
                .iter()
                .map(|e| LacreEntryCr {
                    nome: e.nome.clone(),
                    versao: e.versao.clone(),
                    fonte: serde_json::to_string(&e.fonte).unwrap_or_default(),
                    conteudo: e.conteudo.clone(),
                    fechamento: e.fechamento.clone(),
                    deps_diretas: e.deps_diretas.clone(),
                })
                .collect(),
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        });

        // Upsert — try create, fall back to patch on conflict.
        let pp = PostParams::default();
        let create_result = lacre_api.create(&pp, &cr).await;
        if let Err(KubeError::Api(ae)) = &create_result {
            if ae.code == 409 {
                let patch = Patch::Merge(serde_json::to_value(&cr)?);
                lacre_api
                    .patch(&lacre_name, &PatchParams::apply("caixa-operator"), &patch)
                    .await
                    .map_err(ReconcileError::Kube)?;
            } else {
                return Err(ReconcileError::Kube(KubeError::Api(ae.clone())));
            }
        }
    }

    // Patch Caixa status.
    let api: Api<CaixaCr> = Api::namespaced(ctx.client.clone(), &ns);
    let now = chrono::Utc::now().to_rfc3339();
    let (ready, reason) = if error_msg.is_some() {
        ("False", error_msg.clone().unwrap_or_default())
    } else {
        ("True", "Resolved".into())
    };
    let status = CaixaStatus {
        observed_generation: caixa.metadata.generation,
        resolved_rev: Some(resolved_rev),
        fechamento_root: lacre.as_ref().map(|l| l.raiz.clone()),
        lacre_ref: Some(format!("{name}-lacre")),
        ready: Some(ready.to_string()),
        last_reconciled: Some(now.clone()),
        conditions: vec![Condition {
            kind: "Ready".into(),
            status: ready.to_string(),
            reason: if error_msg.is_some() {
                "Failed".into()
            } else {
                "Resolved".into()
            },
            message: reason,
            last_transition_time: now,
        }],
    };
    let patch = serde_json::json!({
        "apiVersion": "caixa.pleme.io/v1alpha1",
        "kind": "Caixa",
        "status": status,
    });
    api.patch_status(
        &name,
        &PatchParams::apply("caixa-operator").force(),
        &Patch::Apply(&patch),
    )
    .await
    .map_err(ReconcileError::Kube)?;

    Ok(Action::requeue(Duration::from_secs(300)))
}

fn on_error(_caixa: Arc<CaixaCr>, err: &ReconcileError, _ctx: Arc<Context>) -> Action {
    warn!(error = %err, "reconcile failed — requeuing");
    Action::requeue(Duration::from_secs(60))
}

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("kube: {0}")]
    Kube(#[from] KubeError),
    #[error("resolver: {0}")]
    Resolver(#[from] caixa_resolver::ResolveError),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}
