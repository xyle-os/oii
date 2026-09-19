use std::collections::HashMap;
use std::error::Error;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, Hover, HoverContents, HoverParams, HoverProviderCapability,
    InitializeParams, MarkedString, OneOf, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
};

use crate::diag::{Lang, Level};
use crate::{ParseOptions, format_doc, parse_with};

// stdio loop. blocks until shutdown.
pub fn run() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (conn, threads) = Connection::stdio();
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions::default()),
        document_formatting_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        ..Default::default()
    };
    let init: InitializeParams =
        serde_json::from_value(conn.initialize(serde_json::to_value(caps)?)?)?;
    let lang = init
        .initialization_options
        .as_ref()
        .and_then(|v| v.get("lang"))
        .and_then(|v| v.as_str())
        .and_then(Lang::parse)
        .unwrap_or_else(Lang::from_env);
    let mut docs: HashMap<String, String> = HashMap::new();
    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    break;
                }
                if let Some(resp) = on_request(&mut docs, req, lang) {
                    conn.sender.send(Message::Response(resp))?;
                }
            }
            Message::Response(_) => {}
            Message::Notification(note) => {
                if let Some(out) = on_note(&mut docs, note, lang)? {
                    conn.sender.send(out)?;
                }
            }
        }
    }
    // conn holds the writer sender. drop first or join hangs.
    drop(conn);
    threads.join()?;
    Ok(())
}
fn on_request(docs: &mut HashMap<String, String>, req: Request, lang: Lang) -> Option<Response> {
    match req.method.as_str() {
        "textDocument/formatting" => {
            let p: DocumentFormattingParams = serde_json::from_value(req.params).ok()?;
            let text = docs.get(p.text_document.uri.as_str())?;
            let edits = format_edits(text, lang);
            Some(Response::new_ok(
                req.id,
                serde_json::to_value(edits).unwrap_or_default(),
            ))
        }
        "textDocument/completion" => {
            let _p: CompletionParams = serde_json::from_value(req.params).ok()?;
            let r = CompletionResponse::Array(complete_items());
            Some(Response::new_ok(
                req.id,
                serde_json::to_value(r).unwrap_or_default(),
            ))
        }
        "textDocument/hover" => {
            let p: HoverParams = serde_json::from_value(req.params).ok()?;
            let text = docs.get(p.text_document_position_params.text_document.uri.as_str())?;
            let word = word_at(text, p.text_document_position_params.position);
            let md = hover_md(word.as_deref())?;
            let h = Hover {
                contents: HoverContents::Scalar(MarkedString::String(md)),
                range: None,
            };
            Some(Response::new_ok(
                req.id,
                serde_json::to_value(h).unwrap_or_default(),
            ))
        }
        _ => None,
    }
}

fn on_note(
    docs: &mut HashMap<String, String>,
    note: Notification,
    lang: Lang,
) -> Result<Option<Message>, Box<dyn Error + Sync + Send>> {
    match note.method.as_str() {
        "textDocument/didOpen" => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(note.params)?;
            let key = p.text_document.uri.as_str().to_string();
            docs.insert(key.clone(), p.text_document.text.clone());
            Ok(Some(diag_msg(
                &p.text_document.uri,
                &p.text_document.text,
                lang,
            )))
        }
        "textDocument/didChange" => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(note.params)?;
            let key = p.text_document.uri.as_str().to_string();
            if let Some(change) = p.content_changes.into_iter().last() {
                docs.insert(key.clone(), change.text.clone());
                let text = docs.get(&key).cloned().unwrap_or_default();
                return Ok(Some(diag_msg(&p.text_document.uri, &text, lang)));
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn diag_msg(uri: &Uri, text: &str, lang: Lang) -> Message {
    let opts = ParseOptions {
        vars: HashMap::new(),
        lang,
        fix: false,
        // editor has no vars. keep template, skip W001 noise.
        keep_interp: true,
    };
    let out = parse_with(text, &opts);
    let diags = out
        .diagnostics
        .iter()
        .map(|d| Diagnostic {
            range: Range {
                start: Position::new(d.loc.line - 1, d.loc.col - 1),
                end: Position::new(d.loc.end_line - 1, d.loc.end_col - 1),
            },
            severity: Some(match d.level {
                Level::Error => DiagnosticSeverity::ERROR,
                Level::Warning => DiagnosticSeverity::WARNING,
                Level::Fix => DiagnosticSeverity::HINT,
            }),
            code: Some(lsp_types::NumberOrString::String(d.code.to_string())),
            source: Some("oii".to_string()),
            message: match &d.hint {
                Some(h) => format!("{}\nhint: {h}", d.message),
                None => d.message.clone(),
            },
            ..Default::default()
        })
        .collect();
    let p = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: diags,
        version: None,
    };
    Message::Notification(Notification::new(
        "textDocument/publishDiagnostics".to_string(),
        serde_json::to_value(p).unwrap_or_default(),
    ))
}

// full-doc edit. errors mean no edit.
fn format_edits(text: &str, lang: Lang) -> Option<Vec<TextEdit>> {
    let _ = lang;
    let opts = ParseOptions {
        vars: HashMap::new(),
        lang: Lang::Zh,
        fix: false,
        keep_interp: true,
    };
    let out = parse_with(text, &opts);
    if out.has_errors() {
        return None;
    }
    let doc = out.doc?;
    let new_text = format_doc(&doc);
    if new_text == text {
        return Some(vec![]);
    }
    Some(vec![TextEdit {
        range: full_range(text),
        new_text,
    }])
}

fn full_range(text: &str) -> Range {
    let mut line = 0u32;
    let mut col = 0u32;
    for ch in text.chars() {
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Range {
        start: Position::new(0, 0),
        end: Position::new(line, col),
    }
}

fn complete_items() -> Vec<CompletionItem> {
    let mut v = Vec::new();
    let mut add = |label: &str, kind, detail: &str, insert: Option<&str>| {
        v.push(CompletionItem {
            label: label.to_string(),
            kind: Some(kind),
            detail: Some(detail.to_string()),
            insert_text: insert.map(|s| s.to_string()),
            ..Default::default()
        });
    };
    add(
        "impt",
        CompletionItemKind::KEYWORD,
        "imports go first",
        Some("impt \"$0\","),
    );
    add("true", CompletionItemKind::VALUE, "bool", None);
    add("false", CompletionItemKind::VALUE, "bool", None);
    add("null", CompletionItemKind::VALUE, "null", None);
    add(
        "node",
        CompletionItemKind::SNIPPET,
        "name [ attrs ]",
        Some("[$0 [\n  $1\n]"),
    );
    add(
        "attr",
        CompletionItemKind::SNIPPET,
        "key: value",
        Some("$1: $0,"),
    );
    v
}

fn word_at(text: &str, pos: Position) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    let chars: Vec<char> = line.chars().collect();
    let mut i = pos.character as usize;
    if i > 0 && i == chars.len() {
        i -= 1; // cursor past eol. step back.
    }
    let cur = chars.get(i)?;
    if !(cur.is_alphanumeric() || *cur == '_' || *cur == '#') {
        return None;
    }
    let mut a = i;
    while a > 0 && (chars[a - 1].is_alphanumeric() || chars[a - 1] == '_') {
        a -= 1;
    }
    let mut b = i;
    while b + 1 < chars.len() && (chars[b + 1].is_alphanumeric() || chars[b + 1] == '_') {
        b += 1;
    }
    Some(chars[a..=b].iter().collect())
}

fn hover_md(word: Option<&str>) -> Option<String> {
    match word {
        Some("impt") => Some("**impt** — imports go first.\n\n`impt \"a.oii\", \"b.oii\"`".into()),
        Some("true") | Some("false") => Some("bool value".into()),
        Some("null") => Some("null value".into()),
        _ => None,
    }
}

// keep RequestId import used across lsp-server versions
#[allow(dead_code)]
fn _use_id(id: RequestId) -> RequestId {
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_clean_doc() {
        let edits = format_edits("a [ x: 1 ]", Lang::Zh).unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.contains("x: 1"));
    }

    #[test]
    fn no_format_on_error() {
        assert!(format_edits("a [ x: , ]", Lang::Zh).is_none());
    }

    #[test]
    fn keeps_interp_in_editor() {
        let edits = format_edits("m [ t: \"hi {name}\" ]", Lang::Zh).unwrap();
        assert!(edits[0].new_text.contains("{name}"));
    }

    #[test]
    fn diag_range_is_zero_based() {
        let uri: Uri = "file:///x.oii".parse().unwrap();
        let msg = diag_msg(&uri, "a [ x: , ]", Lang::En);
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("E001"));
    }
}
