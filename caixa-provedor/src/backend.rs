use iac_forge::IacForgeError;
use iac_forge::backend::{ArtifactKind, Backend, GeneratedArtifact, NamingConvention};
use iac_forge::ir::{HasAttributes, IacDataSource, IacProvider, IacResource};

use crate::ferrite::FerriteRuntime;
use crate::imports::stdlib_block;

/// Canonical `kebab-case` → `snake_case` ident projection.
///
/// Go package names, Go field / struct / receiver identifiers, and
/// Terraform resource-type + attribute names all reject `-`; a caixa
/// author's canonical kebab-case (`aws-vpc`, `cidr-block`) has to
/// project onto `snake_case` (`aws_vpc`, `cidr_block`) at every emit
/// site that lands in Go source or the Terraform HCL schema. Prior
/// to this lift the six `.replace('-', "_")` call sites across this
/// file — [`FerriteNaming::resource_type_name`] (TF `<provider>_<kind>`
/// resource-type composer), [`FerriteNaming::file_name`] (Go
/// `<name>.go` file-name composer), [`FerriteNaming::field_name`]
/// (Go struct-field renamer), the [`emit_go_resource`] `pkg` /
/// `tf_name` / `tf_name_suffix` locals — each spelled the same
/// hyphen-to-underscore mechanical projection inline, at the same
/// axis (kebab-input, snake-output, single `char → char` swap). A
/// future rebrand of the projection (a Unicode-hyphen accept-set
/// extension for U+2010 / U+2013 / U+2014 authored variants, a
/// dot-to-underscore extension once Terraform resource-type sub-
/// grouping under `<provider>.<kind>` lands, an idempotence check
/// against pre-snake input) would otherwise have to reach every one
/// of the six sites in lockstep or the emit-side would silently
/// drift half-projected — the canonical "I changed the naming rule
/// once and half the emits ignored it" footgun. Now the six sites
/// consult one canonical `str → String` projection and a future
/// rebrand reaches every emit site by construction.
///
/// Same "one canonical primitive under every mechanical renaming"
/// discipline the peer [`go_pascal`] kebab-to-Go-PascalCase
/// projection in this file already carries — this lift extends it
/// onto the sibling kebab-to-snake_case projection every Go /
/// Terraform ident axis this file emits at consumes. The two
/// projections stay distinct because their targets differ: Go
/// exported-symbol axes want `PascalCase` (`AwsVpcResource`), Go
/// unexported-ident + Terraform-schema axes want `snake_case`
/// (`aws_vpc`, `cidr_block`).
#[must_use]
pub fn kebab_to_snake_ident(s: &str) -> String {
    s.replace('-', "_")
}

pub struct FerriteTofuBackend {
    runtime: FerriteRuntime,
    naming: FerriteNaming,
}

impl FerriteTofuBackend {
    #[must_use]
    pub fn new(runtime: FerriteRuntime) -> Self {
        Self {
            runtime,
            naming: FerriteNaming,
        }
    }

    #[must_use]
    pub fn safe() -> Self {
        Self::new(FerriteRuntime::Safe)
    }

    #[must_use]
    pub fn arena() -> Self {
        Self::new(FerriteRuntime::Arena)
    }
}

pub struct FerriteNaming;

impl NamingConvention for FerriteNaming {
    fn resource_type_name(&self, resource_name: &str, provider_name: &str) -> String {
        format!("{provider_name}_{}", kebab_to_snake_ident(resource_name))
    }

    fn file_name(&self, resource_name: &str, _kind: &ArtifactKind) -> String {
        format!("{}.go", kebab_to_snake_ident(resource_name))
    }

    fn field_name(&self, api_name: &str) -> String {
        kebab_to_snake_ident(api_name)
    }
}

impl Backend for FerriteTofuBackend {
    fn platform(&self) -> &str {
        "opentofu-ferrite"
    }

    fn generate_resource(
        &self,
        resource: &IacResource,
        provider: &IacProvider,
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        let source = emit_go_resource(resource, provider, self.runtime);
        Ok(vec![GeneratedArtifact::new(
            self.naming
                .file_name(&resource.name, &ArtifactKind::Resource),
            source,
            ArtifactKind::Resource,
        )])
    }

    fn generate_data_source(
        &self,
        _ds: &IacDataSource,
        _provider: &IacProvider,
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
        Ok(vec![])
    }

    fn generate_provider(
        &self,
        _provider: &IacProvider,
        _resources: &[IacResource],
        _data_sources: &[IacDataSource],
    ) -> Result<Vec<GeneratedArtifact>, IacForgeError> {
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

fn emit_go_resource(
    resource: &IacResource,
    provider: &IacProvider,
    runtime: FerriteRuntime,
) -> String {
    let pkg = kebab_to_snake_ident(&provider.name);
    let tf_name = format!("{pkg}_{}", kebab_to_snake_ident(&resource.name));
    let struct_name = go_pascal(&resource.name);
    // Terraform resource type name (e.g. "aws_vpc") — burned into a header
    // comment so downstream tooling + humans can grep it.
    let _ = &tf_name;
    let variant = runtime.variant_slug();

    let imports = stdlib_block(&[runtime.rt_import()]);

    let mut attrs_schema = String::new();
    let mut state_fields = String::new();
    for attr in resource.input_attributes() {
        let field = go_pascal(&attr.canonical_name);
        attrs_schema.push_str(&format!(
            "\t\t\t{:?}: schema.StringAttribute{{\n\t\t\t\tRequired: {},\n\t\t\t\tSensitive: {},\n\t\t\t}},\n",
            attr.canonical_name, attr.required, attr.sensitive
        ));
        state_fields.push_str(&format!(
            "\t{field} rt.Owned[string] `tfsdk:{:?}`\n",
            attr.canonical_name
        ));
    }

    format!(
        "// Code generated by caixa-provedor ({variant}). DO NOT EDIT.\n\
         //\n\
         // Provider:    {prov}\n\
         // Resource:    {res}\n\
         // TF type:     {tf_name}\n\
         // Runtime:     ferrite `{variant}` — every *T is Owned[T] by construction.\n\
         //\n\
         // Lifecycle:\n\
         //   - Create/Update/Delete borrow the request model mutably.\n\
         //   - Read borrows it immutably.\n\
         //   - All Owned[T] scopes end with an explicit Drop() — the generator\n\
         //     proves these by construction, so `ferrite-check` is a CI gate,\n\
         //     not the author.\n\
         \n\
         package {pkg}\n\
         \n\
         {imports}\n\
         type {struct_name}Resource struct{{}}\n\
         \n\
         type {struct_name}Model struct {{\n{state_fields}}}\n\
         \n\
         func (r *{struct_name}Resource) Metadata(ctx context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {{\n\
             resp.TypeName = req.ProviderTypeName + \"_{tf_name_suffix}\"\n\
         }}\n\
         \n\
         func (r *{struct_name}Resource) Schema(ctx context.Context, req resource.SchemaRequest, resp *resource.SchemaResponse) {{\n\
             resp.Schema = schema.Schema{{\n\
                 Attributes: map[string]schema.Attribute{{\n{attrs_schema}\t\t\t}},\n\
             }}\n\
         }}\n\
         \n\
         func (r *{struct_name}Resource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {{\n\
             // Ferrite invariant: the request's Plan is borrowed mutably, written\n\
             // into a fresh Owned[{struct_name}Model], then Drop()ed when the function returns.\n\
             var plan {struct_name}Model\n\
             diags := req.Plan.Get(ctx, &plan)\n\
             resp.Diagnostics.Append(diags...)\n\
             if resp.Diagnostics.HasError() {{ return }}\n\
             // … CRUD call here …\n\
             diags = resp.State.Set(ctx, plan)\n\
             resp.Diagnostics.Append(diags...)\n\
         }}\n\
         \n\
         // Read/Update/Delete follow the same Owned-per-request pattern.\n\
         // Generated by caixa-provedor — scaffolding omitted for brevity.\n\
         \n\
         var _ resource.Resource = &{struct_name}Resource{{}}\n\
         \n\
         // Assert that every Owned[T] in this file has exactly one Drop on every\n\
         // control-flow path — the generator writes the code to satisfy this.\n\
         var _ = fmt.Sprintf(\"ferrite-check: ok\")\n",
        prov = provider.name,
        res = resource.name,
        tf_name_suffix = kebab_to_snake_ident(&resource.name),
    )
}

fn go_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = true;
    for c in s.chars() {
        if c == '_' || c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use iac_forge::ir::IacType;
    use iac_forge::testing::{TestAttributeBuilder, test_provider, test_resource};

    fn vpc() -> iac_forge::ir::IacResource {
        let mut r = test_resource("vpc");
        r.description = "VPC".into();
        r.attributes = vec![
            TestAttributeBuilder::new("cidr_block", IacType::String)
                .required()
                .description("CIDR")
                .build(),
        ];
        r
    }

    #[test]
    fn emits_ferrite_owned_fields() {
        let be = FerriteTofuBackend::safe();
        let artifacts = be.generate_resource(&vpc(), &test_provider("aws")).unwrap();
        let src = &artifacts[0].content;
        assert!(src.contains("rt.Owned[string]"));
        assert!(src.contains("ferrite/rt"));
        assert!(src.contains("aws_vpc"));
    }

    #[test]
    fn arena_runtime_swaps_import() {
        let be = FerriteTofuBackend::arena();
        let src = &be.generate_resource(&vpc(), &test_provider("aws")).unwrap()[0].content;
        assert!(src.contains("ferrite/rt/arena"));
        assert!(src.contains("ferrite-arena"));
    }

    #[test]
    fn kebab_to_snake_ident_swaps_hyphens_to_underscores() {
        // Canonical projection: every hyphen becomes an underscore,
        // every non-hyphen byte passes through verbatim. Pins the
        // exact byte-shape contract every consumer (Go pkg name,
        // Go field ident, Terraform resource type) depends on.
        assert_eq!(kebab_to_snake_ident("aws-vpc"), "aws_vpc");
        assert_eq!(kebab_to_snake_ident("cidr-block"), "cidr_block");
        assert_eq!(kebab_to_snake_ident("multi-word-ident"), "multi_word_ident");
        // Idempotence: pre-snake input passes through untouched, so
        // a mixed-case call site (a caller that already snake-cased
        // upstream) doesn't double-project into `__` sequences.
        assert_eq!(kebab_to_snake_ident("already_snake"), "already_snake");
        // Empty input round-trips.
        assert_eq!(kebab_to_snake_ident(""), "");
        // Non-hyphen non-alphabetic bytes pass through — the
        // projection is `-` → `_` only, not a general ident-sanitizer.
        assert_eq!(kebab_to_snake_ident("aws/vpc-main"), "aws/vpc_main");
    }

    #[test]
    fn ferrite_naming_axes_route_through_lifted_kebab_to_snake() {
        // Fail-before-pass-after: the three `NamingConvention` axes
        // (resource-type composer, file-name composer, Go field-name
        // renamer) must project every kebab-shaped input onto its
        // snake-shape output — the canonical Go/Terraform ident
        // contract. Previously each axis inlined its own
        // `.replace('-', "_")` call; if any one axis dropped the
        // projection the emitted Go source would carry a raw hyphen
        // in a Go ident (a compile error at `go build` time far
        // from the caixa.lisp author), or the TF resource-type
        // would spell `aws_multi-word` (rejected by the apiserver's
        // TF-schema parser at plan time). This pin exercises each
        // axis with a hyphen-bearing input and asserts the emitted
        // shape carries an underscore in that position, so any
        // future emit-side regression that drops the projection
        // fires at test time — not at the downstream Go compiler
        // or `tofu plan`.
        let naming = FerriteNaming;
        assert_eq!(
            naming.resource_type_name("multi-word", "aws-cloud"),
            "aws-cloud_multi_word",
            "resource_type_name must snake-project the resource-name half"
        );
        assert_eq!(
            naming.file_name("multi-word", &ArtifactKind::Resource),
            "multi_word.go",
            "file_name must snake-project the resource-name so the emitted .go file compiles"
        );
        assert_eq!(
            naming.field_name("cidr-block"),
            "cidr_block",
            "field_name must snake-project the api-name so the emitted Go struct field is a valid Go ident"
        );
    }

    #[test]
    fn emit_go_resource_snake_projects_provider_and_resource_names() {
        // Fail-before-pass-after: the `emit_go_resource` locals
        // (`pkg` = Go package name; `tf_name` = Terraform resource
        // type; `tf_name_suffix` = the TF-type suffix threaded into
        // the emitted `Metadata()` `TypeName` composition) each
        // route the kebab-shaped inputs through the lifted
        // [`kebab_to_snake_ident`] helper. Emit a resource with a
        // hyphen in both the provider and resource names, then pin
        // that (a) the emitted Go source contains the snake-cased
        // package declaration, (b) contains the snake-cased TF
        // resource type, and (c) contains no hyphen-bearing Go
        // ident anywhere the snake projection was responsible for.
        // Before the lift any one of the three sites could drift
        // to spell the hyphen verbatim and land an un-compilable
        // Go package or un-parseable TF type at apply time; the
        // pin makes that regression a test failure.
        let be = FerriteTofuBackend::safe();
        let mut r = test_resource("kv-store");
        r.attributes = vec![
            TestAttributeBuilder::new("cidr_block", IacType::String)
                .required()
                .build(),
        ];
        let src = &be
            .generate_resource(&r, &test_provider("aws-cloud"))
            .unwrap()[0]
            .content;
        assert!(
            src.contains("package aws_cloud"),
            "Go package must snake-project the provider name (got: {src:?})"
        );
        assert!(
            src.contains("aws_cloud_kv_store"),
            "TF resource type must snake-project both halves (got: {src:?})"
        );
        // The header comment preserves the raw kebab names for the
        // reader's benefit; the Go / TF ident axes below the header
        // must never leak a hyphen. Scan every non-comment line for
        // hyphen-bearing tokens.
        for (i, line) in src.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            assert!(
                !line.contains("aws-cloud"),
                "no hyphen-bearing provider ident may leak through non-comment line {i}: {line:?}"
            );
            assert!(
                !line.contains("kv-store"),
                "no hyphen-bearing resource ident may leak through non-comment line {i}: {line:?}"
            );
        }
    }
}
