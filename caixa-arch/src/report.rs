use serde::{Deserialize, Serialize};

use crate::invariants::Violation;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchVerdict {
    /// No safety violations — HCL emission is safe.
    Proven,
    /// Safety violations found — HCL emission must be refused.
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchReport {
    pub verdict: ArchVerdict,
    pub violations: Vec<Violation>,
    pub summary: String,
}

impl ArchReport {
    #[must_use]
    pub fn passed(&self) -> bool {
        matches!(self.verdict, ArchVerdict::Proven)
    }

    #[must_use]
    pub fn safety_count(&self) -> usize {
        self.violations
            .iter()
            .filter(|v| matches!(v.kind, crate::invariants::InvariantKind::Safety))
            .count()
    }
}
