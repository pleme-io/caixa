use iac_forge::IacForgeError;
use iac_forge::backend::{ArtifactKind, Backend, GeneratedArtifact, NamingConvention};
use iac_forge::ir::{IacDataSource, IacProvider, IacResource};

use crate::emit::emit_resource_lisp;
use crate::naming::LispNaming;

pub struct LispBackend {
    naming: LispNaming,
}

impl LispBackend {
    #[must_use]
    pub fn new() -> Self {
        Self { naming: LispNaming }
    }
}

impl Default for LispBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for LispBackend {
    fn platform(&self) -> &str {
        "tatara-lisp"
    }

    fn generate_resource(
        &self,
        resource: &IacResource,
        provider: &IacProvider,
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        let code = emit_resource_lisp(resource, provider);
        Ok(vec![GeneratedArtifact::new(
            self.naming
                .file_name(&resource.name, &ArtifactKind::Resource),
            code,
            ArtifactKind::Resource,
        )])
    }

    fn generate_data_source(
        &self,
        _ds: &IacDataSource,
        _provider: &IacProvider,
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        // Data sources emit as `(defteia-data-source-schema …)` in a later
        // pass. For now, skip silently — return an empty artifact list so
        // callers that iterate every resource + data source don't break.
        Ok(vec![])
    }

    fn generate_provider(
        &self,
        _provider: &IacProvider,
        _resources: &[IacResource],
        _data_sources: &[IacDataSource],
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        // A future pass emits the top-level `caixa.lisp` manifest with deps.
        Ok(vec![])
    }

    fn generate_test(
        &self,
        _resource: &IacResource,
        _provider: &IacProvider,
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        Ok(vec![])
    }

    fn naming(&self) -> &dyn NamingConvention {
        &self.naming
    }
}
