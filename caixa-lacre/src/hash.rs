//! BLAKE3 content and closure hashing — the deterministic identity layer.
//!
//! The format is `blake3:<64 hex chars>`. Labels + length prefixes prevent
//! accidental ambiguity between differently-structured inputs that hash the
//! same raw bytes.

use blake3::Hasher;

/// A length-prefixed, labeled BLAKE3 hasher — accumulates `(label, bytes)`
/// pairs and finalizes to `blake3:<hex>`.
#[derive(Default)]
pub struct ContentHasher {
    h: Hasher,
}

impl ContentHasher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a labeled byte slice. The label + byte length become part of the
    /// hashed stream so `(a, "xy") + (b, "")` differs from `(a, "x") + (b, "y")`.
    pub fn add(&mut self, label: &str, bytes: &[u8]) -> &mut Self {
        self.h.update(label.as_bytes());
        self.h.update(&[0x00]);
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.h.update(&len.to_le_bytes());
        self.h.update(bytes);
        self
    }

    #[must_use]
    pub fn finalize(self) -> String {
        format!("blake3:{}", hex::encode(self.h.finalize().as_bytes()))
    }
}

/// One-shot: hash a single byte slice.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", hex::encode(blake3::hash(bytes).as_bytes()))
}

/// Compute a **closure hash** from this entry's own content hash plus every
/// transitive dep's already-computed closure hash.
///
/// Dep closures are sorted before hashing so the result is order-independent.
#[must_use]
pub fn closure_hash(self_content: &str, dep_closures: &[String]) -> String {
    let mut sorted: Vec<&str> = dep_closures.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    let mut h = ContentHasher::new();
    h.add("self", self_content.as_bytes());
    for d in sorted {
        h.add("dep", d.as_bytes());
    }
    h.finalize()
}

/// Compute the **root hash** of a lacre — a deterministic summary of every
/// entry's name + closure hash, sorted by name.
#[must_use]
pub fn root_hash(entries: &[LacreSummary]) -> String {
    let mut refs: Vec<&LacreSummary> = entries.iter().collect();
    refs.sort_by(|a, b| a.nome.cmp(&b.nome));
    let mut h = ContentHasher::new();
    for e in refs {
        h.add("nome", e.nome.as_bytes());
        h.add("fechamento", e.fechamento.as_bytes());
    }
    h.finalize()
}

/// The bits of a [`crate::LacreEntry`] that contribute to the root hash.
/// Broken out so callers can compute a root without the full entry in hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LacreSummary {
    pub nome: String,
    pub fechamento: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hasher_is_deterministic() {
        let mut a = ContentHasher::new();
        a.add("x", b"hello");
        let ah = a.finalize();

        let mut b = ContentHasher::new();
        b.add("x", b"hello");
        let bh = b.finalize();

        assert_eq!(ah, bh);
        assert!(ah.starts_with("blake3:"));
        assert_eq!(ah.len(), "blake3:".len() + 64);
    }

    #[test]
    fn labels_differentiate_otherwise_equal_inputs() {
        let mut a = ContentHasher::new();
        a.add("x", b"ab");
        let ah = a.finalize();

        let mut b = ContentHasher::new();
        b.add("y", b"ab");
        let bh = b.finalize();

        assert_ne!(ah, bh);
    }

    #[test]
    fn closure_hash_is_order_independent() {
        let self_hash = hash_bytes(b"self");
        let a = hash_bytes(b"a");
        let b = hash_bytes(b"b");
        let c = hash_bytes(b"c");

        let h1 = closure_hash(&self_hash, &[a.clone(), b.clone(), c.clone()]);
        let h2 = closure_hash(&self_hash, &[c, a, b]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn root_hash_ignores_entry_order() {
        let e1 = LacreSummary {
            nome: "a".into(),
            fechamento: hash_bytes(b"a-close"),
        };
        let e2 = LacreSummary {
            nome: "b".into(),
            fechamento: hash_bytes(b"b-close"),
        };

        let r1 = root_hash(&[e1.clone(), e2.clone()]);
        let r2 = root_hash(&[e2, e1]);
        assert_eq!(r1, r2);
    }

    #[test]
    fn root_hash_differs_with_fechamento_change() {
        let a = LacreSummary {
            nome: "x".into(),
            fechamento: hash_bytes(b"v1"),
        };
        let b = LacreSummary {
            nome: "x".into(),
            fechamento: hash_bytes(b"v2"),
        };
        assert_ne!(root_hash(&[a]), root_hash(&[b]));
    }
}
