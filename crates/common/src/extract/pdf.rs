// Post-MVP: PDF text extraction via pdf-extract crate.
// Stub returns no lines until implemented.

use std::path::Path;
use crate::api::IndexLine;
use crate::extract::Extractor;

pub struct PdfExtractor;

impl Extractor for PdfExtractor {
    fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("pdf"))
            .unwrap_or(false)
    }

    fn extract(&self, _path: &Path) -> anyhow::Result<Vec<IndexLine>> {
        Ok(vec![])
    }
}
