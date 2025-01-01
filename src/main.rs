use anyhow::Result;
use directories;
use lsp_server::{Connection, Message, Request, Response};
use lsp_types::notification::{self, Notification as TypesNotification};
use lsp_types::request::{self, Request as TypesRequest};
use lsp_types::{
    CompletionItem, CompletionOptions, CompletionParams, CompletionResponse, ServerCapabilities,
    TextDocumentSyncKind,
};
use regex::Regex;
use std::collections::HashSet;
use std::sync::Mutex;
use std::{fs, path::Path};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

fn create_log_file(base_path: &Path) -> anyhow::Result<fs::File> {
    let dir_path = base_path.join("lsp-word");
    fs::create_dir_all(&dir_path)?;
    let file_path = dir_path.join("lsp-word.log");
    Ok(fs::File::create(file_path)?)
}

fn init_logger() {
    let builder = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_env("LSP_WORD_LOG"))
        .with_max_level(tracing::Level::INFO);
    let base_dirs = directories::BaseDirs::new();
    match base_dirs.and_then(|base| create_log_file(base.cache_dir()).ok()) {
        Some(log_file) => builder.with_writer(Mutex::new(log_file)).init(),
        _ => builder.with_writer(std::io::stderr).without_time().init(),
    }
}

fn load_all_words(uri: lsp_types::Uri) -> Result<HashSet<String>> {
    let path = uri.path().as_str();
    let content = fs::read_to_string(path)?;
    Ok(Regex::new(r"[A-Za-z_][A-Za-z0-9_]+")?
        .find_iter(&content)
        .map(|m| m.as_str().to_owned())
        .collect::<HashSet<String>>())
}

fn create_completion_response(req: Request) -> Result<Message> {
    let params = serde_json::from_value::<CompletionParams>(req.params)?;
    let words = load_all_words(params.text_document_position.text_document.uri)?;
    let compres = CompletionResponse::Array(
        words
            .iter()
            .map(|word| CompletionItem {
                label: word.to_owned(),
                ..Default::default()
            })
            .collect(),
    );
    let result = serde_json::to_value(compres).ok();
    Ok(Message::Response(Response {
        id: req.id,
        result: result,
        error: None,
    }))
}

fn serve(connection: Connection) -> Result<()> {
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => match req.method.as_str() {
                request::Shutdown::METHOD => {
                    connection.handle_shutdown(&req)?;
                }
                request::Completion::METHOD => {
                    connection.sender.send(create_completion_response(req)?)?
                }
                _ => (),
            },
            Message::Notification(not) => match not.method.as_str() {
                notification::Exit::METHOD => (),
                notification::DidChangeTextDocument::METHOD => (),
                _ => (),
            },
            _ => (),
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    init_logger();
    info!("Starting LSP server");
    let (connection, io_threads) = Connection::stdio();

    let triggers: Vec<String> = ('A'..='Z')
        .chain('a'..='z')
        .map(|c| c.to_string())
        .collect();

    let server_capabilities = serde_json::to_value(ServerCapabilities {
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(triggers),
            ..Default::default()
        }),
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        ..Default::default()
    })?;

    match connection.initialize(server_capabilities) {
        Ok(initialize_result) => {
            let params = serde_json::to_string(&initialize_result)?;
            info!("Initialized with params: {}", params);
            if let Err(e) = serve(connection) {
                error!("{e:?}");
            }
        }
        Err(err) => {
            error!("Error initializing connection: {:?}", err);
            return Ok(());
        }
    }
    Ok(io_threads.join()?)
}
