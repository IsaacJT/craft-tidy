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
        Self::start_with(json!({}))
    }

    /// Starts a server for a client with the given capabilities.
    fn start_with(capabilities: Value) -> Self {
        let (server, connection) = Connection::memory();
        let server = std::thread::spawn(move || lsp::serve(server));
        let mut client = Client {
            connection,
            server: Some(server),
            next_id: 1,
        };
        let result = client.request("initialize", json!({"capabilities": capabilities}));
        assert_eq!(
            result["capabilities"]["textDocumentSync"],
            json!({"openClose": true, "change": 1, "save": true})
        );
        assert_eq!(result["capabilities"]["hoverProvider"], true);
        assert_eq!(
            result["capabilities"]["codeActionProvider"]["codeActionKinds"],
            json!(["quickfix", "source.fixAll"])
        );
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

// Code actions

/// The code actions offered for `range` (`[start line, start character,
/// end line, end character]`) of `uri`, limited to the kinds in `only`.
fn code_actions(
    client: &mut Client,
    uri: &str,
    range: [u32; 4],
    only: Option<&[&str]>,
) -> Vec<Value> {
    let mut context = json!({"diagnostics": []});
    if let Some(only) = only {
        context["only"] = json!(only);
    }
    let result = client.request(
        "textDocument/codeAction",
        json!({
            "textDocument": {"uri": uri},
            "range": {
                "start": {"line": range[0], "character": range[1]},
                "end": {"line": range[2], "character": range[3]},
            },
            "context": context,
        }),
    );
    result.as_array().unwrap().clone()
}

/// `text` with the single edit of `action` applied.
fn apply(text: &str, uri: &str, action: &Value) -> String {
    let edits = action["edit"]["changes"][uri].as_array().unwrap();
    assert_eq!(edits.len(), 1);
    let edit = &edits[0];
    let lines = LineIndex::new(text);
    let position = |p: &Value| {
        lines.offset_of(Position::new(
            p["line"].as_u64().unwrap() as u32,
            p["character"].as_u64().unwrap() as u32,
        ))
    };
    let range = position(&edit["range"]["start"])..position(&edit["range"]["end"]);
    let mut result = text.to_owned();
    result.replace_range(range, edit["newText"].as_str().unwrap());
    result
}

fn titles(actions: &[Value]) -> Vec<(String, String)> {
    actions
        .iter()
        .map(|a| {
            (
                a["kind"].as_str().unwrap().to_owned(),
                a["title"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// A client with `text` open as a project file, and its URI.
fn open_project(text: &str) -> (Client, String, tempfile::TempDir) {
    let dir = project(Some("checks: -style-collapsed-sequences\n"));
    let file = uri(&dir.path().join("snap/snapcraft.yaml"));
    let client = Client::start();
    client.open(&file, text);
    client.diagnostics();
    (client, file, dir)
}

#[test]
fn offers_fixes_and_suppressions_for_findings_at_the_cursor() {
    let text = "name: test\nplugs: [b, a]\nslots: [d, c]\n";
    let (mut client, file, _dir) = open_project(text);
    let actions = code_actions(&mut client, &file, [1, 9, 1, 9], None);
    assert_eq!(
        titles(&actions),
        [
            (
                "quickfix".to_owned(),
                "Fix: entries of 'plugs' are not sorted: 'a' should come before 'b'".to_owned()
            ),
            (
                "quickfix".to_owned(),
                "Suppress style-sorted-lists on this line with NOLINTNEXTLINE".to_owned()
            ),
            (
                "source.fixAll".to_owned(),
                "Fix all craft-tidy problems".to_owned()
            ),
        ]
    );
    assert_eq!(actions[0]["isPreferred"], true);
    assert_eq!(actions[0]["diagnostics"][0]["code"], "style-sorted-lists");
    // Each fixes what it says, and only that.
    assert_eq!(
        apply(text, &file, &actions[0]),
        "name: test\nplugs: [a, b]\nslots: [d, c]\n"
    );
    assert_eq!(
        apply(text, &file, &actions[1]),
        "name: test\n# NOLINTNEXTLINE(style-sorted-lists)\nplugs: [b, a]\nslots: [d, c]\n"
    );
    assert_eq!(
        apply(text, &file, &actions[2]),
        "name: test\nplugs: [a, b]\nslots: [c, d]\n"
    );
    // The fix is a minimal edit of the list, not of the whole file.
    let edit = &actions[0]["edit"]["changes"][&file][0];
    assert_eq!(edit["range"]["start"], json!({"line": 1, "character": 8}));
    assert_eq!(edit["range"]["end"], json!({"line": 1, "character": 12}));
    client.shutdown();
}

#[test]
fn a_selection_covers_every_finding_in_it() {
    let text = "plugs: [b, a]\nslots: [d, c]\n";
    let (mut client, file, _dir) = open_project(text);
    let actions = code_actions(&mut client, &file, [0, 0, 2, 0], Some(&["quickfix"]));
    assert_eq!(actions.len(), 4);
    // Away from any finding, only fixing the whole file is offered.
    let text = "name: test\nplugs: [b, a]\n";
    client.change(&file, 2, text);
    client.diagnostics();
    let actions = code_actions(&mut client, &file, [0, 2, 0, 2], None);
    assert_eq!(titles(&actions)[0].0, "source.fixAll");
    assert_eq!(actions.len(), 1);
    client.shutdown();
}

#[test]
fn only_limits_the_kinds_of_actions() {
    let text = "plugs: [b, a]\n";
    let (mut client, file, _dir) = open_project(text);
    let kinds = |client: &mut Client, only: &[&str]| -> Vec<String> {
        titles(&code_actions(client, &file, [0, 9, 0, 9], Some(only)))
            .into_iter()
            .map(|(kind, _)| kind)
            .collect()
    };
    assert_eq!(kinds(&mut client, &["source.fixAll"]), ["source.fixAll"]);
    assert_eq!(kinds(&mut client, &["source"]), ["source.fixAll"]);
    assert_eq!(kinds(&mut client, &["quickfix"]), ["quickfix", "quickfix"]);
    assert!(kinds(&mut client, &["refactor"]).is_empty());
    assert!(kinds(&mut client, &["sourcefoo"]).is_empty());
    client.shutdown();
}

#[test]
fn findings_that_cannot_be_fixed_can_still_be_suppressed() {
    let text = "name: test\nparts:\n  app:\n    plugin: dump\n    source: .\n";
    let (mut client, file, _dir) = open_project(text);
    let actions = code_actions(&mut client, &file, [4, 12, 4, 12], None);
    assert_eq!(
        titles(&actions),
        [(
            "quickfix".to_owned(),
            "Suppress project-root-source on this line with NOLINTNEXTLINE".to_owned()
        )]
    );
    // The comment is indented like the line.
    assert_eq!(
        apply(text, &file, &actions[0]),
        "name: test\nparts:\n  app:\n    plugin: dump\n    \
         # NOLINTNEXTLINE(project-root-source)\n    source: .\n"
    );
    client.shutdown();
}

#[test]
fn does_not_offer_suppressions_that_would_not_work() {
    // A comment inside a literal block would be part of the text.
    let long = "x".repeat(90);
    let text =
        format!("name: test\nparts:\n  app:\n    override-build: |\n      echo\n      {long}\n");
    let (mut client, file, _dir) = open_project(&text);
    let actions = code_actions(&mut client, &file, [5, 85, 5, 85], Some(&["quickfix"]));
    assert!(actions.is_empty(), "{actions:?}");
    // Mistakes in NOLINT comments are not suppressed with NOLINT.
    let text = "name: test\n# NOLINTEND\n";
    client.change(&file, 2, text);
    client.diagnostics();
    let actions = code_actions(&mut client, &file, [1, 3, 1, 3], Some(&["quickfix"]));
    assert!(actions.is_empty(), "{actions:?}");
    client.shutdown();
}

#[test]
fn code_actions_need_an_open_checked_document() {
    let mut client = Client::start();
    let dir = project(None);
    let file = uri(&dir.path().join("snap/snapcraft.yaml"));
    assert!(code_actions(&mut client, &file, [0, 0, 0, 0], None).is_empty());
    // Invalid YAML cannot be fixed.
    client.open(&file, "plugs: [b, a\n");
    client.diagnostics();
    assert!(code_actions(&mut client, &file, [0, 0, 0, 0], None).is_empty());
    client.shutdown();
}

// Hover

fn hover(client: &mut Client, uri: &str, line: u32, character: u32) -> Value {
    client.request(
        "textDocument/hover",
        json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}}),
    )
}

#[test]
fn hovering_over_a_finding_explains_its_check() {
    let text = "name: test\nplugs: [b, a]\n";
    let (mut client, file, _dir) = open_project(text);
    let result = hover(&mut client, &file, 1, 9);
    assert_eq!(result["contents"]["kind"], "markdown");
    let value = result["contents"]["value"].as_str().unwrap();
    assert!(
        value.starts_with(
            "```text\nstyle-sorted-lists: Entries of unordered lists must be sorted alphabetically\n"
        ),
        "{value}"
    );
    assert!(
        value.contains("\nOptions:\n  keys (string, default: "),
        "{value}"
    );
    assert_eq!(
        result["range"],
        json!({"start": {"line": 1, "character": 8}, "end": {"line": 1, "character": 12}})
    );
    // Nothing to explain elsewhere.
    assert_eq!(hover(&mut client, &file, 0, 2), Value::Null);
    client.shutdown();
}

#[test]
fn hover_explains_every_check_with_a_finding_there() {
    let dir = project(None);
    let file = uri(&dir.path().join("snap/snapcraft.yaml"));
    let mut client = Client::start();
    client.open(&file, "plugs: [b, a]\n");
    client.diagnostics();
    let result = hover(&mut client, &file, 0, 9);
    let value = result["contents"]["value"].as_str().unwrap();
    let sections: Vec<_> = value.split("\n\n---\n\n").collect();
    assert_eq!(sections.len(), 2);
    assert!(sections[0].starts_with("```text\nstyle-collapsed-sequences: "));
    assert!(sections[1].starts_with("```text\nstyle-sorted-lists: "));
    // The range covers both findings.
    assert_eq!(
        result["range"],
        json!({"start": {"line": 0, "character": 7}, "end": {"line": 0, "character": 13}})
    );
    client.shutdown();
}

// Configuration changes

#[test]
fn asks_editors_that_can_to_watch_configuration_files() {
    let client = Client::start_with(
        json!({"workspace": {"didChangeWatchedFiles": {"dynamicRegistration": true}}}),
    );
    let Message::Request(request) = client.receive() else {
        panic!("expected a registration request");
    };
    assert_eq!(request.method, "client/registerCapability");
    let registration = &request.params["registrations"][0];
    assert_eq!(registration["method"], "workspace/didChangeWatchedFiles");
    assert_eq!(
        registration["registerOptions"]["watchers"],
        json!([{"globPattern": "**/.craft-tidy"}])
    );
    client
        .connection
        .sender
        .send(Response::new_ok(request.id, Value::Null).into())
        .unwrap();
    client.shutdown();

    // Other editors are not asked: the next message answers a request.
    let mut client = Client::start();
    let response = client.response("craft-tidy/unknown", Value::Null);
    assert!(response.response_result.is_err());
    client.shutdown();
}

#[test]
fn rechecks_open_documents_when_the_configuration_changes() {
    let dir = project(None);
    let file = uri(&dir.path().join("snap/snapcraft.yaml"));
    let config = dir.path().join(".craft-tidy");
    let client = Client::start();
    client.open(&file, "plugs: [b, a]\n");
    assert_eq!(client.diagnostics().2.len(), 2);

    // A change reported by file watching.
    fs::write(&config, "checks: -style-sorted-lists\n").unwrap();
    client.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": uri(&config), "type": 2}]}),
    );
    let (published, version, diagnostics) = client.diagnostics();
    assert_eq!((published.as_str(), version), (file.as_str(), Some(1)));
    assert_eq!(
        summary(&diagnostics),
        [("style-collapsed-sequences".to_owned(), 0, 7, 0, 13)]
    );

    // The configuration file saved in the editor.
    fs::write(&config, "checks: -style-*\n").unwrap();
    client.notify(
        "textDocument/didSave",
        json!({"textDocument": {"uri": uri(&config)}}),
    );
    assert!(client.diagnostics().2.is_empty());

    // Saving other files changes nothing: the next message answers a
    // request.
    let mut client = client;
    client.notify(
        "textDocument/didSave",
        json!({"textDocument": {"uri": file}}),
    );
    let response = client.response("craft-tidy/unknown", Value::Null);
    assert!(response.response_result.is_err());
    client.shutdown();
}
