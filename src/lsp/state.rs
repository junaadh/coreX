use crate::lsp::convert::uri_to_path;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct OpenDocument {
    pub path: PathBuf,
    pub text: String,
    pub version: Option<i64>,
}

#[derive(Debug, Default)]
pub struct ServerState {
    shutdown_requested: bool,
    documents_by_uri: BTreeMap<String, OpenDocument>,
}

impl ServerState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark_shutdown_requested(&mut self) {
        self.shutdown_requested = true;
    }

    pub fn upsert_open_document(
        &mut self,
        uri: String,
        text: String,
        version: Option<i64>,
    ) -> Result<(), String> {
        let Some(path) = uri_to_path(&uri) else {
            return Err(format!("unsupported non-file URI: {uri}"));
        };
        self.documents_by_uri.insert(
            uri.clone(),
            OpenDocument {
                path,
                text,
                version,
            },
        );
        Ok(())
    }

    pub fn update_open_document(
        &mut self,
        uri: &str,
        text: String,
        version: Option<i64>,
    ) -> Result<(), String> {
        let Some(document) = self.documents_by_uri.get_mut(uri) else {
            return Err(format!("didChange for unopened document: {uri}"));
        };
        document.text = text;
        document.version = version;
        Ok(())
    }

    pub fn close_document(&mut self, uri: &str) {
        self.documents_by_uri.remove(uri);
    }

    #[must_use]
    pub fn document(&self, uri: &str) -> Option<&OpenDocument> {
        self.documents_by_uri.get(uri)
    }

    #[must_use]
    pub fn open_text_by_path(&self) -> BTreeMap<PathBuf, String> {
        self.documents_by_uri
            .values()
            .map(|document| (document.path.clone(), document.text.clone()))
            .collect()
    }
}
