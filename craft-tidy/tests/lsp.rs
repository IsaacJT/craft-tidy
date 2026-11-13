// SPDX-License-Identifier: GPL-3.0-only

//! Tests for the language server (`craft_tidy::lsp`).

mod common;

use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::thread::JoinHandle;
use std::time::Duration;

use craft_tidy::lsp::{self, LineIndex};
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{Position, Uri};
use serde_json::{Value, json};

use common::project;

/// A client connected to a server running in a thread.
struct Client {
    connection: Connection,
    server: Option<JoinHandle<Result<(), String>>>,
    next_id: i32,
}

impl Client {
    fn start() -> Self {
        let (server, connection) = Connection::memory();
        let server = std::thread::spawn(move || lsp::serve(server));
        let mut client = Client {
            connection,
            server: Some(server),
            next_id: 1,
        };
        let result = client.request("initialize", json!({"capabilities": {}}));
        assert_eq!(result["capabilities"]["textDocumentSync"], 1);
        client.notify("initialized", json!({}));
        client
    }

    fn notify(&self, method: &str, params: Value) {
        self.connection
            .sender
            .send(Notification::new(method.to_owned(), params).into())
            .unwrap();
    }

    fn send_request(&mut self, method: &str, params: Value) -> RequestId {
        let id = RequestId::from(self.next_id);
        self.next_id += 1;
        self.connection
            .sender
            .send(Request::new(id.clone(), method.to_owned(), params).into())
            .unwrap();
        id
    }

    /// The response to a request, failing on an error response.
    fn request(&mut self, method: &str, params: Value) -> Value {
        match self.response(method, params).response_result {
            Ok(result) => result,
            Err(error) => panic!("{method} failed: {}", error.message),
        }
    }

    fn response(&mut self, method: &str, params: Value) -> Response {
        let id = self.send_request(method, params);
        loop {
            match self.receive() {
                Message::Response(response) if response.id == id => return response,
                _ => {}
            }
        }
    }

    fn receive(&self) -> Message {
        self.connection
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the server should answer")
    }

    /// The next diagnostics published, as `(uri, version, diagnostics)`.
    fn diagnostics(&self) -> (String, Option<i64>, Vec<Value>) {
        loop {
            if let Message::Notification(notification) = self.receive()
                && notification.method == "textDocument/publishDiagnostics"
            {
                let params = notification.params;
                return (
                    params["uri"].as_str().unwrap().to_owned(),
                    params["version"].as_i64(),
                    params["diagnostics"].as_array().unwrap().clone(),
                );
            }
        }
    }

    fn open(&self, uri: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": "yaml", "version": 1, "text": text}}),
        );
    }

    fn change(&self, uri: &str, version: i32, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}),
        );
    }

    fn shutdown(mut self) {
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        let server = self.server.take().unwrap();
        assert_eq!(server.join().unwrap(), Ok(()));
    }
}

fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// `(check, start line, start character, end line, end character)`.
fn summary(diagnostics: &[Value]) -> Vec<(String, u64, u64, u64, u64)> {
    diagnostics
        .iter()
        .map(|d| {
            assert_eq!(d["source"], "craft-tidy");
            let range = &d["range"];
            (
                d["code"].as_str().unwrap().to_owned(),
                range["start"]["line"].as_u64().unwrap(),
                range["start"]["character"].as_u64().unwrap(),
                range["end"]["line"].as_u64().unwrap(),
                range["end"]["character"].as_u64().unwrap(),
            )
        })
        .collect()
}

// Diagnostics

#[test]
fn publishes_findings_of_open_documents() {
    let dir = project(None);
    let file = uri(&dir.path().join("snap/snapcraft.yaml"));
    let client = Client::start();
    client.open(&file, "name: test\nplugs: [b, a]\n");
    let (published, version, diagnostics) = client.diagnostics();
    assert_eq!((published.as_str(), version), (file.as_str(), Some(1)));
    assert_eq!(
        summary(&diagnostics),
        [
            ("style-collapsed-sequences".to_owned(), 1, 7, 1, 13),
            ("style-sorted-lists".to_owned(), 1, 8, 1, 12),
        ]
    );
    assert_eq!(diagnostics[1]["severity"], 2);
    assert_eq!(
        diagnostics[1]["message"],
        "entries of 'plugs' are not sorted: 'a' should come before 'b'"
    );

    // Edits are checked as they are made, before saving.
    client.change(&file, 2, "name: test\nplugs: [a]\n");
    let (_, version, diagnostics) = client.diagnostics();
    assert_eq!(version, Some(2));
    assert!(diagnostics.is_empty());

    // Closing a document clears its findings.
    client.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": file}}),
    );
    let (_, version, diagnostics) = client.diagnostics();
    assert_eq!(version, None);
    assert!(diagnostics.is_empty());
    client.shutdown();
}

#[test]
fn uses_the_configuration_of_the_file() {
    let dir = project(Some(
        "checks: -style-collapsed-sequences\nwarnings-as-errors: style-*\n",
    ));
    let file = uri(&dir.path().join("snap/snapcraft.yaml"));
    let client = Client::start();
    client.open(&file, "plugs: [b, a]\n");
    let (_, _, diagnostics) = client.diagnostics();
    assert_eq!(
        summary(&diagnostics),
        [("style-sorted-lists".to_owned(), 0, 8, 0, 12)]
    );
    assert_eq!(diagnostics[0]["severity"], 1);
    client.shutdown();
}

#[test]
fn reports_configuration_and_syntax_errors_in_the_file() {
    let dir = project(Some("checks: [1]\n"));
    let file = uri(&dir.path().join("snap/snapcraft.yaml"));
    let client = Client::start();
    client.open(&file, "name: test\n");
    let (_, _, diagnostics) = client.diagnostics();
    assert_eq!(
        summary(&diagnostics),
        [("craft-tidy-config".to_owned(), 0, 0, 0, 0)]
    );
    assert_eq!(diagnostics[0]["severity"], 1);
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap()
            .starts_with("invalid configuration in ")
    );

    let dir = project(None);
    let file = uri(&dir.path().join("snap/snapcraft.yaml"));
    client.open(&file, "name: test\nparts: [a\n");
    let (_, _, diagnostics) = client.diagnostics();
    assert_eq!(
        summary(&diagnostics),
        [("craft-tidy-syntax".to_owned(), 1, 7, 1, 7)]
    );
    assert_eq!(
        diagnostics[0]["message"],
        "invalid YAML: unclosed bracket '['"
    );
    client.shutdown();
}

#[test]
fn checks_project_files_of_every_tool_only() {
    let dir = tempfile::tempdir().unwrap();
    let client = Client::start();
    for name in ["rockcraft.yaml", "charmcraft.yaml", ".snapcraft.yaml"] {
        let file = uri(&dir.path().join(name));
        client.open(&file, "plugs: [b, a]\n");
        let (published, _, diagnostics) = client.diagnostics();
        assert_eq!(published, file);
        assert!(!diagnostics.is_empty(), "{name}");
    }
    // Other YAML files and documents that are not files are ignored: the
    // next message is the response to a later request.
    let mut client = client;
    client.open(&uri(&dir.path().join("other.yaml")), "plugs: [b, a]\n");
    client.open("untitled:Untitled-1", "plugs: [b, a]\n");
    let response = client.response("craft-tidy/unknown", Value::Null);
    assert!(response.response_result.is_err());
    client.shutdown();
}

#[test]
fn rejects_unsupported_requests() {
    let mut client = Client::start();
    let response = client.response("textDocument/definition", json!({}));
    let error = response.response_result.unwrap_err();
    assert_eq!(error.code, lsp_server::ErrorCode::MethodNotFound as i32);
    client.shutdown();
}

// Positions and URIs

#[test]
fn converts_offsets_to_utf16_positions() {
    let text = "a: é\nb: 😀x\r\nc\n";
    let lines = LineIndex::new(text);
    assert_eq!(lines.position(0), Position::new(0, 0));
    assert_eq!(lines.position(3), Position::new(0, 3));
    // é is two bytes but one UTF-16 unit; 😀 is four bytes and two units.
    assert_eq!(lines.position(5), Position::new(0, 4));
    let x = text.find('x').unwrap();
    assert_eq!(lines.position(x), Position::new(1, 5));
    assert_eq!(lines.position(text.find('c').unwrap()), Position::new(2, 0));
    // Offsets inside a character or past the end are clamped.
    assert_eq!(lines.position(4), Position::new(0, 3));
    assert_eq!(lines.position(1000), Position::new(3, 0));

    for offset in [0, 3, 6, x, text.find('c').unwrap()] {
        assert_eq!(lines.offset_of(lines.position(offset)), offset);
    }
    // Positions past the end of a line are clamped to it.
    assert_eq!(
        lines.offset_of(Position::new(0, 99)),
        text.find('\n').unwrap()
    );
    assert_eq!(lines.offset_of(Position::new(99, 0)), text.len());
    // 1-based line and character columns, as in findings.
    assert_eq!(lines.offset(2, 5), x);
}

#[test]
fn converts_file_uris_to_paths() {
    let path = |s: &str| lsp::uri_to_path(&Uri::from_str(s).unwrap());
    assert_eq!(
        path("file:///home/me/snap/snapcraft.yaml").as_deref(),
        Some(Path::new("/home/me/snap/snapcraft.yaml"))
    );
    assert_eq!(
        path("file:///home/me/my%20project/caf%C3%A9/rockcraft.yaml").as_deref(),
        Some(Path::new("/home/me/my project/café/rockcraft.yaml"))
    );
    assert_eq!(
        path("file://localhost/srv/snapcraft.yaml").as_deref(),
        Some(Path::new("/srv/snapcraft.yaml"))
    );
    for other in [
        "untitled:Untitled-1",
        "https://example.com/snapcraft.yaml",
        "file://server/share/snapcraft.yaml",
        // Not UTF-8 once decoded.
        "file:///bad%FF",
    ] {
        assert_eq!(path(other), None, "{other}");
    }
}

#[test]
fn configuration_files_need_not_exist() {
    // Files of a project that is not on disk yet are checked with the
    // defaults.
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("new/snap/snapcraft.yaml");
    assert!(!missing.parent().unwrap().exists());
    let client = Client::start();
    client.open(&uri(&missing), "plugs: [b, a]\n");
    let (_, _, diagnostics) = client.diagnostics();
    assert_eq!(diagnostics.len(), 2);
    client.shutdown();
    fs::remove_dir_all(dir.path()).unwrap();
}
