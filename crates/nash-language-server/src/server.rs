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
