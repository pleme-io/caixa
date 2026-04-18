use iac_forge::backend::{ArtifactKind, NamingConvention};

/// Lisp-flavored naming — kebab-case files, `<provider>/<resource>` types.
pub struct LispNaming;

impl NamingConvention for LispNaming {
    fn resource_type_name(&self, resource_name: &str, provider_name: &str) -> String {
        format!("{provider_name}/{}", kebab(resource_name))
    }

    fn file_name(&self, resource_name: &str, _kind: &ArtifactKind) -> String {
        format!("{}.lisp", kebab(resource_name))
    }

    fn field_name(&self, api_name: &str) -> String {
        kebab(api_name)
    }
}

fn kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            for lc in c.to_lowercase() {
                out.push(lc);
            }
        } else if c == '_' {
            out.push('-');
        } else {
            out.push(c);
        }
    }
    out
}
