//! Import block templating.

#[must_use]
pub fn stdlib_block(extra_imports: &[&str]) -> String {
    let mut out = String::from("import (\n");
    out.push_str("\t\"context\"\n");
    out.push_str("\t\"fmt\"\n");
    out.push('\n');
    out.push_str("\t\"github.com/hashicorp/terraform-plugin-framework/resource\"\n");
    out.push_str("\t\"github.com/hashicorp/terraform-plugin-framework/resource/schema\"\n");
    out.push('\n');
    for line in extra_imports {
        out.push_str(&format!("\t{line}\n"));
    }
    out.push_str(")\n");
    out
}
