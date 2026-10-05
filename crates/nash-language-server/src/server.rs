use tokio::sync::Mutex;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, InitializedParams, MessageType, ServerInfo, Uri,
};
use tower_lsp_server::{Client, LanguageServer};
use url::Url;

use crate::capabilities::server_capabilities;
use crate::workspace::Workspace;

pub const SERVER_NAME: &str = "nash-language-server";

pub struct Server {
    client: Client,
    // Hold through publication: an old build cannot overtake a newer edit.
    workspace: Mutex<Workspace>,
}

impl Server {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace: Mutex::new(Workspace::default()),
        }
    }

    pub fn server_info() -> ServerInfo {
        ServerInfo {
            name: SERVER_NAME.to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }
    }

    async fn publish(&self, workspace: &mut Workspace, uri: &Uri) {
        let (notifications, error) = workspace.rebuild(uri).await;
        for notification in notifications {
            self.client
                .publish_diagnostics(
                    notification.uri,
                    notification.diagnostics,
                    notification.version,
                )
                .await;
        }
        if let Some(error) = error {
            self.client.log_message(MessageType::ERROR, error).await;
        }
    }
}

impl LanguageServer for Server {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let mut workspace = self.workspace.lock().await;
        workspace.roots = params
            .workspace_folders
            .unwrap_or_default()
            .into_iter()
            .filter_map(|folder| Url::parse(folder.uri.as_str()).ok()?.to_file_path().ok())
            .map(|path| path.canonicalize().unwrap_or(path))
            .collect();
        #[allow(deprecated)]
        if workspace.roots.is_empty()
            && let Some(uri) = params.root_uri
            && let Ok(url) = Url::parse(uri.as_str())
            && let Ok(path) = url.to_file_path()
        {
            workspace.roots.push(path.canonicalize().unwrap_or(path));
        }
        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(Self::server_info()),
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "nash language server initialized")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        let mut workspace = self.workspace.lock().await;
        if workspace.open(document.uri.clone(), document.text, document.version) {
            self.publish(&mut workspace, &document.uri).await;
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        if change.range.is_some() {
            return;
        } // FULL synchronization is advertised.
        let mut workspace = self.workspace.lock().await;
        if workspace.change(
            &params.text_document.uri,
            change.text,
            params.text_document.version,
        ) {
            self.publish(&mut workspace, &params.text_document.uri)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut workspace = self.workspace.lock().await;
        workspace.close(&params.text_document.uri);
        self.publish(&mut workspace, &params.text_document.uri)
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::LspService;
    use tower_lsp_server::ls_types::WorkspaceFolder;

    #[tokio::test]
    #[allow(deprecated)]
    async fn initialization_preserves_root_uri_fallback_and_workspace_priority() {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path().canonicalize().unwrap();
        for name in ["legacy", "first", "second"] {
            std::fs::create_dir(base.join(name)).unwrap();
        }
        let uri = |name: &str| {
            Url::from_directory_path(base.join(name))
                .unwrap()
                .as_str()
                .parse::<Uri>()
                .unwrap()
        };
        let folder = |name: &str| WorkspaceFolder {
            name: name.to_owned(),
            uri: uri(name),
        };
        let invalid_uri: Uri = "https://example.invalid/workspace".parse().unwrap();
        let invalid_folder = WorkspaceFolder {
            name: "non-file".into(),
            uri: invalid_uri.clone(),
        };
        let cases = [
            ("absent folders", None, Some(uri("legacy"))),
            ("empty folders", Some(vec![]), Some(uri("legacy"))),
            (
                "unusable folders",
                Some(vec![invalid_folder.clone()]),
                Some(uri("legacy")),
            ),
            (
                "workspace folder overrides legacy root",
                Some(vec![folder("first")]),
                Some(uri("legacy")),
            ),
            ("unusable legacy root", None, Some(invalid_uri)),
            (
                "multiple folders retain order",
                Some(vec![folder("second"), invalid_folder, folder("first")]),
                Some(uri("legacy")),
            ),
            ("no roots clears previous initialization", None, None),
        ];
        let (service, _socket) = LspService::new(Server::new);
        let mut output = String::new();
        for (label, workspace_folders, root_uri) in cases {
            service
                .inner()
                .initialize(InitializeParams {
                    workspace_folders,
                    root_uri,
                    ..InitializeParams::default()
                })
                .await
                .expect("initialization succeeds");
            let workspace = service.inner().workspace.lock().await;
            let roots: Vec<_> = workspace
                .roots
                .iter()
                .map(|path| {
                    path.strip_prefix(&base)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            output.push_str(&format!("{label}: {roots:?}\n"));
        }
        insta::assert_snapshot!(output);
    }
}
