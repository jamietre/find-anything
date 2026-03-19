use std::sync::Arc;

/// Opaque content-addressable key (wraps the blake3 hex hash).
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContentKey(Arc<str>);

impl ContentKey {
    pub fn new(hash: impl Into<Arc<str>>) -> Self {
        Self(hash.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
