use anyhow::Result;
use directories;
use lsp_server::{Connection, Message, Request, Response};
use lsp_types::notification::{self, Notification as TypesNotification};
use lsp_types::request::{self, Request as TypesRequest};
use lsp_types::{
    CompletionItem, CompletionOptions, CompletionParams, CompletionResponse, ServerCapabilities,
    TextDocumentSyncKind, Uri,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::{fs, path::Path};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

fn create_log_file(base_path: &Path) -> anyhow::Result<fs::File> {
    let dir_path = base_path.join("lsp-word");
    fs::create_dir_all(&dir_path)?;
    let file_path = dir_path.join("lsp-word.log");
    Ok(fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?)
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

fn load_all_words(uri: Uri, docs: &HashMap<Uri, String>) -> Result<HashSet<String>> {
    let content = docs.get(&uri).expect("Document not found");
    Ok(Regex::new(r"[A-Za-z_][A-Za-z0-9_]+")?
        .find_iter(&content)
        .map(|m| m.as_str().to_owned())
        .collect::<HashSet<String>>())
}

fn create_completion_response(req: Request, docs: &HashMap<Uri, String>) -> Result<Message> {
    let params = serde_json::from_value::<CompletionParams>(req.params)?;
    let words = load_all_words(params.text_document_position.text_document.uri, docs)?;
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
        result,
        error: None,
    }))
}

fn serve(connection: Connection) -> Result<()> {
    let mut docs = HashMap::new();
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => match req.method.as_str() {
                request::Shutdown::METHOD => {
                    connection.handle_shutdown(&req)?;
                }
                request::Completion::METHOD => connection
                    .sender
                    .send(create_completion_response(req, &docs)?)?,
                _ => (),
            },
            Message::Notification(not) => match not.method.as_str() {
                notification::Exit::METHOD => (),
                notification::DidChangeTextDocument::METHOD => {
                    let params = serde_json::from_value::<lsp_types::DidChangeTextDocumentParams>(
                        not.params,
                    )?;

                    docs.insert(
                        params.text_document.uri.to_owned(),
                        params.content_changes[0].text.clone(),
                    );
                }
                notification::DidOpenTextDocument::METHOD => {
                    let params =
                        serde_json::from_value::<lsp_types::DidOpenTextDocumentParams>(not.params)?;
                    docs.insert(
                        params.text_document.uri.to_owned(),
                        params.text_document.text,
                    );
                }
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
            TextDocumentSyncKind::FULL,
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

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{TextDocumentIdentifier, TextDocumentPositionParams};
    use std::collections::HashMap;

    #[test]
    fn test_create_log_file() {
        let temp_dir = std::env::temp_dir();
        let file = create_log_file(&temp_dir).unwrap();
        assert!(file.metadata().unwrap().is_file());
        let log_path = temp_dir.join("lsp-word").join("lsp-word.log");
        assert!(log_path.exists());
    }

    #[test]
    fn test_load_all_words_basic() {
        let uri = "file:///test".parse::<Uri>().unwrap();
        let mut docs = HashMap::new();
        docs.insert(uri.clone(), "fn main() { let test = 1; }".to_string());

        let words = load_all_words(uri, &docs).unwrap();
        let expected_words: HashSet<String> = ["fn", "main", "let", "test"]
            .iter()
            .cloned()
            .map(String::from)
            .collect();

        assert_eq!(words, expected_words);
    }

    #[test]
    fn test_load_all_words_empty() {
        let uri = "file:///test".parse::<Uri>().unwrap();
        let mut docs = HashMap::new();
        docs.insert(uri.clone(), "".to_string());

        let words = load_all_words(uri, &docs).unwrap();
        assert!(words.is_empty());
    }

    #[test]
    fn test_load_all_words_special_chars() {
        let uri = "file:///test".parse::<Uri>().unwrap();
        let mut docs = HashMap::new();
        docs.insert(uri.clone(), "let x1 = 42; // @#$%".to_string());

        let words = load_all_words(uri, &docs).unwrap();
        let expected_words: HashSet<String> =
            ["let", "x1"].iter().cloned().map(String::from).collect();

        assert_eq!(words, expected_words);
    }

    #[test]
    fn test_create_completion_response() {
        let uri = "file:///test".parse::<Uri>().unwrap();
        let mut docs = HashMap::new();
        docs.insert(uri.clone(), "fn main() { let test = 1; }".to_string());

        let req = Request {
            id: 1.into(),
            method: "textDocument/completion".to_string(),
            params: serde_json::to_value(CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: lsp_types::Position {
                        line: 0,
                        character: 0,
                    },
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            })
            .unwrap(),
        };

        let response = create_completion_response(req, &docs).unwrap();
        if let Message::Response(resp) = response {
            assert!(resp.result.is_some());
        } else {
            panic!("Expected a response message");
        }
    }
}
