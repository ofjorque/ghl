pub mod backend;
pub mod intel;

pub use backend::Backend;
use tower_lsp::{LspService, Server};

/// Run the GHL Language Server on standard I/O.
pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
