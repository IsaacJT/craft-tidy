// SPDX-License-Identifier: GPL-3.0-only

//! A language server (LSP), so that editors can show craft-tidy's findings
//! as the project file is edited.
//!
//! Run with `craft-tidy lsp`; it talks to the editor over standard input
//! and output. Open project files (recognised by their name, such as
//! `snapcraft.yaml`) are checked as they change, with the configuration
//! found for them as on the command line.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand,
    CodeActionProviderCapability, DiagnosticSeverity, NumberOrString, Position,
    PublishDiagnosticsParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextEdit, Uri, WorkspaceEdit,
};

use crate::app;
use crate::checks::{self, Check, Severity};
use crate::config::Settings;
use crate::craft::Craft;
use crate::error::Error;
use crate::nolint;
use crate::project::Project;

/// The `source` of the diagnostics the server publishes.
pub const SOURCE: &str = "craft-tidy";

/// The code of diagnostics about the configuration or the YAML syntax,
/// which are not findings of a check.
pub const CONFIG_CODE: &str = "craft-tidy-config";
pub const SYNTAX_CODE: &str = "craft-tidy-syntax";

/// Runs the server on standard input and output until the editor shuts it
/// down.
pub fn run_stdio() -> Result<(), String> {
    let (connection, threads) = Connection::stdio();
    serve(connection)?;
    threads.join().map_err(|error| error.to_string())
}

/// Runs the server on `connection` until the editor shuts it down.
pub fn serve(connection: Connection) -> Result<(), String> {
    let capabilities = serde_json::to_value(capabilities()).map_err(|e| e.to_string())?;
    connection
        .initialize(capabilities)
        .map_err(|error| error.to_string())?;
    let mut server = Server {
        connection: &connection,
        registry: checks::registry(),
        documents: HashMap::new(),
    };
    server.main_loop()
}

fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![
                CodeActionKind::QUICKFIX,
                CodeActionKind::SOURCE_FIX_ALL,
            ]),
            ..CodeActionOptions::default()
        })),
        ..ServerCapabilities::default()
    }
}

/// An open document.
struct OpenDocument {
    text: String,
    version: i32,
}

struct Server<'a> {
    connection: &'a Connection,
    registry: Vec<Box<dyn Check>>,
    /// Open documents by URI.
    documents: HashMap<String, OpenDocument>,
}

impl Server<'_> {
    fn main_loop(&mut self) -> Result<(), String> {
        for message in &self.connection.receiver {
            match message {
                Message::Request(request) => {
                    if self
                        .connection
                        .handle_shutdown(&request)
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(());
                    }
                    let response = self.handle_request(request);
                    self.send(response.into())?;
                }
                Message::Notification(notification) => self.handle_notification(notification)?,
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn send(&self, message: Message) -> Result<(), String> {
        self.connection
            .sender
            .send(message)
            .map_err(|error| error.to_string())
    }

    fn handle_request(&mut self, request: Request) -> Response {
        use lsp_types::request::{CodeActionRequest, Request as _};
        if request.method == CodeActionRequest::METHOD {
            let params: lsp_types::CodeActionParams = match parse(request.params) {
                Ok(params) => params,
                Err(error) => {
                    return Response::new_err(request.id, ErrorCode::InvalidParams as i32, error);
                }
            };
            let uri = &params.text_document.uri;
            let actions = match self.documents.get(uri.as_str()) {
                Some(document) => code_actions(
                    &self.registry,
                    uri,
                    &document.text,
                    params.range,
                    params.context.only.as_deref(),
                ),
                None => Vec::new(),
            };
            return Response::new_ok(request.id, actions);
        }
        Response::new_err(
            request.id,
            ErrorCode::MethodNotFound as i32,
            format!("unsupported request {}", request.method),
        )
    }

    fn handle_notification(&mut self, notification: Notification) -> Result<(), String> {
        use lsp_types::notification::{
            DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
        };
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: lsp_types::DidOpenTextDocumentParams = parse(notification.params)?;
                let document = params.text_document;
                self.documents.insert(
                    document.uri.as_str().to_owned(),
                    OpenDocument {
                        text: document.text,
                        version: document.version,
                    },
                );
                self.publish(&document.uri)
            }
            DidChangeTextDocument::METHOD => {
                let params: lsp_types::DidChangeTextDocumentParams = parse(notification.params)?;
                // With full synchronisation, the last change holds the
                // whole text.
                if let Some(change) = params.content_changes.into_iter().last() {
                    self.documents.insert(
                        params.text_document.uri.as_str().to_owned(),
                        OpenDocument {
                            text: change.text,
                            version: params.text_document.version,
                        },
                    );
                }
                self.publish(&params.text_document.uri)
            }
            DidCloseTextDocument::METHOD => {
                let params: lsp_types::DidCloseTextDocumentParams = parse(notification.params)?;
                let uri = params.text_document.uri;
                self.documents.remove(uri.as_str());
                self.send_diagnostics(uri, Vec::new(), None)
            }
            _ => Ok(()),
        }
    }

    /// Checks the open document `uri` and publishes its findings.
    fn publish(&self, uri: &Uri) -> Result<(), String> {
        let Some(document) = self.documents.get(uri.as_str()) else {
            return Ok(());
        };
        let diagnostics = diagnostics(&self.registry, uri, &document.text).unwrap_or_default();
        self.send_diagnostics(uri.clone(), diagnostics, Some(document.version))
    }

    fn send_diagnostics(
        &self,
        uri: Uri,
        diagnostics: Vec<lsp_types::Diagnostic>,
        version: Option<i32>,
    ) -> Result<(), String> {
        use lsp_types::notification::{Notification as _, PublishDiagnostics};
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        };
        self.send(Message::Notification(Notification::new(
            PublishDiagnostics::METHOD.to_owned(),
            params,
        )))
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(params).map_err(|error| error.to_string())
}

/// The result of checking a document.
enum Analysis {
    /// Not a craft project file.
    NotProject,
    /// The configuration or the YAML is invalid, so it could not be
    /// checked.
    Invalid(lsp_types::Diagnostic),
    Checked {
        project: Project,
        settings: Settings,
        findings: Vec<checks::Diagnostic>,
    },
}

/// Checks the document `uri` with contents `text`.
fn analyse(registry: &[Box<dyn Check>], uri: &Uri, text: &str) -> Analysis {
    let Some(path) = uri_to_path(uri).filter(|path| Craft::from_path(path).is_some()) else {
        return Analysis::NotProject;
    };
    let lines = LineIndex::new(text);
    let invalid = |code: &str, offset: usize, message: String| {
        Analysis::Invalid(lsp_types::Diagnostic {
            range: lines.range(offset..offset),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(code.to_owned())),
            source: Some(SOURCE.to_owned()),
            message,
            ..lsp_types::Diagnostic::default()
        })
    };
    let settings = match app::settings_for(&path).and_then(|settings| {
        checks::validate_options(registry, &settings.check_options)?;
        Ok(settings)
    }) {
        Ok(settings) => settings,
        Err(error) => return invalid(CONFIG_CODE, 0, error.to_string()),
    };
    let project = match Project::new(path, text) {
        Ok(project) => project,
        Err(Error::Syntax { error, .. }) => {
            let offset = lines.offset(error.line, error.column);
            return invalid(
                SYNTAX_CODE,
                offset,
                format!("invalid YAML: {}", error.message),
            );
        }
        Err(error) => return invalid(SYNTAX_CODE, 0, error.to_string()),
    };
    let enabled = checks::enabled(registry, &settings);
    let findings = checks::run(&enabled, &project, &settings);
    Analysis::Checked {
        project,
        settings,
        findings,
    }
}

/// The findings for the document `uri` with contents `text`, or `None` if
/// it is not a craft project file.
pub fn diagnostics(
    registry: &[Box<dyn Check>],
    uri: &Uri,
    text: &str,
) -> Option<Vec<lsp_types::Diagnostic>> {
    match analyse(registry, uri, text) {
        Analysis::NotProject => None,
        Analysis::Invalid(diagnostic) => Some(vec![diagnostic]),
        Analysis::Checked { findings, .. } => {
            let lines = LineIndex::new(text);
            Some(findings.iter().map(|f| to_lsp(f, &lines)).collect())
        }
    }
}

/// The byte range of the source a finding is about.
fn span_of(finding: &checks::Diagnostic, lines: &LineIndex<'_>) -> Range<usize> {
    match (&finding.span, finding.location) {
        (Some(span), _) => span.clone(),
        (None, Some((line, column))) => {
            let offset = lines.offset(line, column);
            offset..offset
        }
        (None, None) => 0..0,
    }
}

fn to_lsp(finding: &checks::Diagnostic, lines: &LineIndex<'_>) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: lines.range(span_of(finding, lines)),
        severity: Some(match finding.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(finding.check.to_owned())),
        source: Some(SOURCE.to_owned()),
        message: finding.message.clone(),
        ..lsp_types::Diagnostic::default()
    }
}

/// The code actions for the byte range `range` of the document `uri` with
/// contents `text`: a quick fix and a suppression for each finding there,
/// and fixing all findings of the file. `only`, if given, limits the kinds
/// of actions returned.
pub fn code_actions(
    registry: &[Box<dyn Check>],
    uri: &Uri,
    text: &str,
    range: lsp_types::Range,
    only: Option<&[CodeActionKind]>,
) -> Vec<CodeActionOrCommand> {
    let Analysis::Checked {
        project,
        settings,
        findings,
    } = analyse(registry, uri, text)
    else {
        return Vec::new();
    };
    let wanted = |kind: &CodeActionKind| {
        only.is_none_or(|only| {
            only.iter().any(|prefix| {
                kind.as_str() == prefix.as_str()
                    || kind.as_str().starts_with(&format!("{}.", prefix.as_str()))
            })
        })
    };
    let lines = LineIndex::new(text);
    let requested = lines.offset_of(range.start)..lines.offset_of(range.end);
    let action = |title: String,
                  kind: CodeActionKind,
                  fixed: &str,
                  diagnostics: Option<Vec<lsp_types::Diagnostic>>,
                  preferred: bool| {
        let edit = minimal_edit(text, fixed, &lines);
        CodeActionOrCommand::CodeAction(CodeAction {
            title,
            kind: Some(kind),
            diagnostics,
            edit: Some(WorkspaceEdit {
                changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
                ..WorkspaceEdit::default()
            }),
            is_preferred: preferred.then_some(true),
            ..CodeAction::default()
        })
    };

    let mut actions = Vec::new();
    if wanted(&CodeActionKind::QUICKFIX) {
        for finding in &findings {
            let Some(span) = &finding.span else {
                continue;
            };
            // The finding is at the cursor, or within the selection.
            if span.start > requested.end || requested.start > span.end {
                continue;
            }
            let diagnostics = Some(vec![to_lsp(finding, &lines)]);
            if let Some(check) = registry.iter().find(|c| c.name() == finding.check)
                && let Some(fixed) =
                    checks::fix_span(check.as_ref(), &project, &settings, span.clone())
            {
                actions.push(action(
                    format!("Fix: {}", finding.message),
                    CodeActionKind::QUICKFIX,
                    fixed.source(),
                    diagnostics.clone(),
                    true,
                ));
            }
            if let Some(suppressed) = suppress(registry, &project, &settings, &findings, finding) {
                actions.push(action(
                    format!(
                        "Suppress {} on this line with NOLINTNEXTLINE",
                        finding.check
                    ),
                    CodeActionKind::QUICKFIX,
                    &suppressed,
                    diagnostics,
                    false,
                ));
            }
        }
    }
    if wanted(&CodeActionKind::SOURCE_FIX_ALL) {
        let enabled = checks::enabled(registry, &settings);
        if let Some(fixed) = checks::fix(&enabled, &project, &settings) {
            actions.push(action(
                "Fix all craft-tidy problems".to_owned(),
                CodeActionKind::SOURCE_FIX_ALL,
                fixed.source(),
                None,
                false,
            ));
        }
    }
    actions
}

/// The source of `project` with `finding` suppressed by a
/// `# NOLINTNEXTLINE(check)` comment on the line above it, if that keeps
/// the data unchanged and suppresses exactly that finding. (Comments
/// cannot be added inside block scalars, for example.)
fn suppress(
    registry: &[Box<dyn Check>],
    project: &Project,
    settings: &Settings,
    findings: &[checks::Diagnostic],
    finding: &checks::Diagnostic,
) -> Option<String> {
    if finding.check == nolint::NAME {
        return None;
    }
    let document = &project.document;
    let source = document.source();
    let line = document.line_index(finding.span.as_ref()?.start);
    let start = document.line_range(line).start;
    let indent: String = document
        .line(line)
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let comment = format!(
        "{indent}# NOLINTNEXTLINE({}){}",
        finding.check,
        document.newline()
    );
    let suppressed = format!("{}{comment}{}", &source[..start], &source[start..]);
    let edited = Project::new(project.path.clone(), suppressed.as_str()).ok()?;
    if edited.document.value() != document.value() {
        return None;
    }
    let enabled = checks::enabled(registry, settings);
    let remaining = checks::run(&enabled, &edited, settings);
    let gone = |f: &checks::Diagnostic| f.check == finding.check && f.message == finding.message;
    let before = findings.iter().filter(|f| gone(f)).count();
    let after = remaining.iter().filter(|f| gone(f)).count();
    (remaining.len() + 1 == findings.len() && after + 1 == before).then_some(suppressed)
}

/// A single edit turning `old` into `new`: the replacement of the part
/// between their common prefix and suffix.
fn minimal_edit(old: &str, new: &str, lines: &LineIndex<'_>) -> TextEdit {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let max_suffix = old.len().min(new.len()) - prefix;
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    TextEdit::new(
        lines.range(prefix..old.len() - suffix),
        new[prefix..new.len() - suffix].to_owned(),
    )
}

/// The local path of a `file:` URI.
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    // The authority must be empty or localhost.
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    let mut bytes = Vec::with_capacity(path.len());
    let mut chars = path.bytes();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            let hex = [chars.next()?, chars.next()?];
            let hex = std::str::from_utf8(&hex).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    let path = String::from_utf8(bytes).ok()?;
    Some(Path::new(&path).to_owned())
}

/// Converts byte offsets of a text to LSP positions, whose columns count
/// UTF-16 code units.
pub struct LineIndex<'a> {
    text: &'a str,
    /// Byte offsets of the start of each line.
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(offset, _)| offset + 1));
        Self { text, starts }
    }

    /// The LSP position of byte `offset` (clamped to the text, and to the
    /// start of a character).
    pub fn position(&self, offset: usize) -> Position {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        let column = self.text[self.starts[line]..offset].encode_utf16().count();
        Position::new(line as u32, column as u32)
    }

    pub fn range(&self, span: Range<usize>) -> lsp_types::Range {
        lsp_types::Range::new(self.position(span.start), self.position(span.end))
    }

    /// The byte offset of an LSP `position`, clamped to its line.
    pub fn offset_of(&self, position: Position) -> usize {
        let Some(&start) = self.starts.get(position.line as usize) else {
            return self.text.len();
        };
        let end = self
            .starts
            .get(position.line as usize + 1)
            .map_or(self.text.len(), |&next| next - 1);
        let mut units = 0;
        for (index, c) in self.text[start..end].char_indices() {
            if units >= position.character as usize {
                return start + index;
            }
            units += c.len_utf16();
        }
        end
    }

    /// The byte offset of a 1-based line and character column, as used in
    /// craft-tidy's findings and YAML errors.
    pub fn offset(&self, line: usize, column: usize) -> usize {
        let Some(&start) = self.starts.get(line.saturating_sub(1)) else {
            return self.text.len();
        };
        self.text[start..]
            .char_indices()
            .nth(column.saturating_sub(1))
            .map_or(self.text.len(), |(index, _)| start + index)
    }
}
