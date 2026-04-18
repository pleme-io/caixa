//! Ferrite runtime flavor + import helpers.

/// Which ferrite runtime to target at emit time.
///
/// `Safe` → `ferrite/rt` (checked types, GC-compatible). The same binary
/// passes `ferrite-check` and can later be swapped to `Arena` via
/// `ferrite-mutate`.
///
/// `Arena` → `ferrite/rt/arena` (mmap regions, zero-GC). Final form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FerriteRuntime {
    Safe,
    Arena,
}

#[must_use]
pub fn ferrite_rt_import(runtime: FerriteRuntime) -> &'static str {
    match runtime {
        FerriteRuntime::Safe => "rt \"github.com/pleme-io/ferrite/rt\"",
        FerriteRuntime::Arena => "rt \"github.com/pleme-io/ferrite/rt/arena\"",
    }
}

/// Convenience: the `runtime` flavor as the short string used in file headers.
#[must_use]
pub const fn ferrite_runtime_variant(runtime: FerriteRuntime) -> &'static str {
    match runtime {
        FerriteRuntime::Safe => "ferrite-safe",
        FerriteRuntime::Arena => "ferrite-arena",
    }
}
