use std::path::{Path, PathBuf};

use crate::ast::{Doc, Node, Value};
use crate::diag::Lang;
use crate::lex::{self, Kind};
use crate::{ParseOptions, parse_with};

type Span = (usize, usize);

// docfile keeps the source text edits splice exact byte ranges so comments and
// layout survive the ast is reparsed after each edit
pub struct DocFile {
    pub path: Option<PathBuf>,
    text: String,
    doc: Doc,
}

impl DocFile {
    pub fn load(path: impl AsRef<Path>) -> Result<DocFile, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("read {} failed: {e}", path.display()))?;
        let mut f = DocFile::parse(&text)?;
        f.path = Some(path.to_path_buf());
        Ok(f)
    }

    pub fn parse(text: &str) -> Result<DocFile, String> {
        let out = parse_with(
            text,
            &ParseOptions {
                vars: Default::default(),
                lang: Lang::En,
                fix: false,
                keep_interp: true,
            },
        );
        match out.doc {
            Some(doc) => Ok(DocFile {
                path: None,
                text: text.to_string(),
                doc,
            }),
            None => {
                let msg = out
                    .diagnostics
                    .iter()
                    .filter(|d| d.level == crate::diag::Level::Error)
                    .map(|d| d.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ");
                Err(format!("parse failed: {msg}"))
            }
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    pub fn get(&self, path: &str) -> Option<&Value> {
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        let (head, rest) = segs.split_first()?;
        let node = self.doc.node(head)?;
        lookup_in_node(node, rest)
    }

    // replace an attribute value in place creates the attr when missing
    pub fn set(&mut self, path: &str, value: &Value) -> Result<(), String> {
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        if segs.is_empty() {
            return Err("empty path".to_string());
        }
        // func.desc has its own path
        if segs.len() == 2 && segs[1] == "desc" && self.doc.func(segs[0]).is_some() {
            return match self.attr_value_span(&segs) {
                Some(span) => {
                    let rendered = crate::fmt::fmt_value(value);
                    self.splice(span, &rendered)
                }
                None => Err("func has no desc slot to set".to_string()),
            };
        }
        // array element replace. e.g. config.registry.extra -> new value
        if segs.len() >= 3 {
            let arr_segs = &segs[..segs.len() - 1];
            let want = segs[segs.len() - 1];
            let found = self
                .array_span(arr_segs)
                .and_then(|(span, _)| self.array_elem_span(span, want));
            if let Some((a, b)) = found {
                let rendered = crate::fmt::fmt_value(value);
                return self.splice((a, b), &rendered);
            }
        }
        match self.attr_value_span(&segs) {
            Some(span) => {
                let rendered = crate::fmt::fmt_value(value);
                self.splice(span, &rendered)?;
            }
            None => {
                if segs.len() < 2 {
                    return Err(format!("no attribute at `{path}`"));
                }
                let key = segs[segs.len() - 1].to_string();
                let parent = segs[..segs.len() - 1].join(".");
                self.add_attr(&parent, &key, value)?;
            }
        }
        Ok(())
    }

    pub fn set_desc(&mut self, func: &str, desc: &str) -> Result<(), String> {
        let flats = top_layout(&self.text);
        let fl = flats
            .funcs
            .iter()
            .find(|f| f.name == func)
            .ok_or_else(|| format!("no func `{func}`"))?;
        let rendered = crate::fmt::fmt_value(&Value::Str(desc.to_string()));
        match find_desc_span(&self.text, fl.open_start) {
            Some(span) => self.splice(span, &rendered)?,
            None => {
                let at = token_end(&self.text, fl.open_start, &Kind::LBracket);
                let pad = indent_unit(&self.text).repeat(fl.depth + 1);
                let insert = format!("\n{pad}desc: {rendered},");
                self.splice((at, at), &insert)?;
            }
        }
        Ok(())
    }

    pub fn add_attr(&mut self, path: &str, key: &str, value: &Value) -> Result<(), String> {
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        let (name, rest) = segs.split_first().ok_or("empty path")?;
        let layout = top_layout(&self.text);
        let node =
            find_node(&layout.nodes, name, rest).ok_or_else(|| format!("no node `{path}`"))?;
        let close = node.close_span.ok_or("node has no body")?;
        let open = node.open_span.ok_or("node has no body")?;
        let rendered = format!("{key}: {}", crate::fmt::fmt_value(value));
        let unit = indent_unit(&self.text);
        let (open_line, _) = crate::diag::line_col(&self.text, open.0);
        let (close_line, _) = crate::diag::line_col(&self.text, close.0);
        if open_line == close_line {
            if self.text[open.1..close.0].trim().is_empty() {
                // empty inline body. open it up into lines
                let pad = unit.repeat(node.depth + 1);
                let close_pad = unit.repeat(node.depth);
                let new = format!("[\n{pad}{rendered}\n{close_pad}]");
                self.splice((open.0, close.1), &new)?;
            } else {
                let at = close.0;
                self.splice((at, at), &format!(" {rendered},"))?;
            }
        } else {
            let pad = unit.repeat(node.depth + 1);
            let at = line_start(&self.text, close.0);
            let insert = format!("{pad}{rendered},\n");
            self.splice((at, at), &insert)?;
        }
        Ok(())
    }

    pub fn remove_node(&mut self, path: &str) -> Result<(), String> {
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        let (name, rest) = segs.split_first().ok_or("empty path")?;
        let layout = top_layout(&self.text);
        let node =
            find_node(&layout.nodes, name, rest).ok_or_else(|| format!("no node `{path}`"))?;
        let (a, b) = node.full_span;
        let a = line_start(&self.text, a);
        let b = if self.text[b..].starts_with('\n') {
            b + 1
        } else {
            b
        };
        self.splice((a, b), "")?;
        Ok(())
    }

    // insert a raw line or snippet into a func body, an array attr, or a node
    // body. target is a func name or a dot path. raw is inserted verbatim then
    // the whole doc is reparsed. bad raw rolls the whole insert back
    pub fn insert(&mut self, target: &str, raw: &str) -> Result<(), String> {
        self.transaction(|f| f.insert_impl(target, raw))
    }

    fn insert_impl(&mut self, target: &str, raw: &str) -> Result<(), String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("empty line".to_string());
        }
        let layout = top_layout(&self.text);
        if let Some(f) = layout.funcs.iter().find(|f| f.name == target) {
            let (open, close_start, depth) = (f.open_span, f.close_start, f.depth);
            // statements do not swallow the next statement on a newline
            return self.insert_body(open, close_start, depth, raw, " ");
        }
        let segs: Vec<&str> = target.split('.').filter(|s| !s.is_empty()).collect();
        if segs.is_empty() {
            return Err("empty target".to_string());
        }
        // array attribute. append an element
        if let Some((span, depth)) = self.array_span(&segs) {
            return self.insert_array(span, depth, raw);
        }
        // node body. terminate a trailing bare sibling so it does not swallow
        if let Some((_, _, _, Some(at))) = self.node_body(&segs) {
            let c = self.text[at..].chars().next();
            if !matches!(c, Some(',') | Some(']')) {
                self.splice((at, at), ",")?;
            }
        }
        if let Some((open, close, depth, _)) = self.node_body(&segs) {
            return self.insert_body(open, close.0, depth, raw, ", ");
        }
        Err(format!("no func or node at `{target}`"))
    }

    // add a whole node. None appends at top level
    pub fn add_node(&mut self, parent: Option<&str>, src: &str) -> Result<(), String> {
        match parent {
            Some(p) => self.insert(p, src),
            None => self.append_top(src),
        }
    }

    // append a top level func
    pub fn add_func(&mut self, src: &str) -> Result<(), String> {
        self.append_top(src)
    }

    // remove a func, a node, or an attribute. path picks the first match
    pub fn remove(&mut self, path: &str) -> Result<(), String> {
        if self.doc.func(path).is_some() {
            let layout = top_layout(&self.text);
            if let Some(f) = layout.funcs.iter().find(|f| f.name == path) {
                let (a, b) = f.full_span;
                return self.remove_line_span(a, b);
            }
        }
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        if segs.is_empty() {
            return Err("empty path".to_string());
        }
        // func statement line. e.g. packages.git
        if segs.len() == 2 && self.doc.func(segs[0]).is_some() {
            if let Some((a, b)) = self.func_stmt_span(segs[0], segs[1]) {
                return self.remove_line_span(a, b);
            }
        }
        // array element by value. e.g. config.registry.aur
        if segs.len() >= 3 {
            let arr_segs = &segs[..segs.len() - 1];
            let want = segs[segs.len() - 1];
            if let Some((span, _)) = self.array_span(arr_segs) {
                if let Some((a, b)) = self.array_elem_span(span, want) {
                    return self.remove_line_span(a, b);
                }
            }
        }
        let layout = top_layout(&self.text);
        if let Some(n) = find_node(&layout.nodes, segs[0], &segs[1..]) {
            let (a, b) = n.full_span;
            return self.remove_line_span(a, b);
        }
        if let Some(attr) = self.find_attr(&segs) {
            let (a, b) = attr.full_span;
            return self.remove_line_span(a, b);
        }
        Err(format!("no func node or attr at `{path}`"))
    }

    // insert a raw line before an existing entry. anchor is a func statement, a
    // node child, an attr key, or an array element value
    pub fn insert_before(&mut self, target: &str, anchor: &str, raw: &str) -> Result<(), String> {
        self.transaction(|f| f.insert_before_impl(target, anchor, raw))
    }

    fn insert_before_impl(&mut self, target: &str, anchor: &str, raw: &str) -> Result<(), String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("empty line".to_string());
        }
        let unit = indent_unit(&self.text);
        let layout = top_layout(&self.text);
        if let Some(f) = layout.funcs.iter().find(|f| f.name == target) {
            let depth = f.depth;
            if let Some(span) = self.func_stmt_span(target, anchor) {
                let pad = unit.repeat(depth + 1);
                return self.insert_line_before(span, raw, &pad, false);
            }
            return Err(format!("no statement `{anchor}` in func `{target}`"));
        }
        let segs: Vec<&str> = target.split('.').filter(|s| !s.is_empty()).collect();
        if segs.is_empty() {
            return Err("empty target".to_string());
        }
        // array element
        if let Some((span, depth)) = self.array_span(&segs) {
            if let Some(espan) = self.array_elem_span(span, anchor) {
                let pad = unit.repeat(depth + 2);
                return self.insert_line_before(espan, raw, &pad, true);
            }
            return Err(format!("no element `{anchor}` in `{target}`"));
        }
        // node child or attr. terminate a preceding bare sibling first
        let anchor_span = self.node_child_span(&segs, anchor);
        if let Some(span) = anchor_span {
            if let Some(at) = self.prev_bare_end(&segs, span.0) {
                let c = self.text[at..].chars().next();
                if !matches!(c, Some(',') | Some(']')) {
                    self.splice((at, at), ",")?;
                }
            }
            if let Some(span) = self.node_child_span(&segs, anchor) {
                let depth = self.node_depth(&segs).unwrap_or(0);
                let pad = unit.repeat(depth + 1);
                return self.insert_line_before(span, raw, &pad, true);
            }
        }
        Err(format!("no func or node at `{target}`"))
    }

    // replace the array element at index
    pub fn set_index(&mut self, path: &str, i: usize, value: &Value) -> Result<(), String> {
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        let span = self
            .array_span(&segs)
            .ok_or_else(|| format!("no array at `{path}`"))?
            .0;
        let elems = self.array_elem_spans(span);
        let e = *elems
            .get(i)
            .ok_or_else(|| format!("index {i} out of range"))?;
        let rendered = crate::fmt::fmt_value(value);
        self.splice(e, &rendered)
    }

    // insert an array element at index. i past the end appends
    pub fn insert_index(&mut self, path: &str, i: usize, raw: &str) -> Result<(), String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("empty line".to_string());
        }
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        let (span, depth) = self
            .array_span(&segs)
            .ok_or_else(|| format!("no array at `{path}`"))?;
        let elems = self.array_elem_spans(span);
        if i >= elems.len() {
            return self.insert_array(span, depth, raw);
        }
        let unit = indent_unit(&self.text);
        let pad = unit.repeat(depth + 2);
        self.insert_line_before(elems[i], raw, &pad, true)
    }

    // remove the array element at index
    pub fn remove_index(&mut self, path: &str, i: usize) -> Result<(), String> {
        let segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        let span = self
            .array_span(&segs)
            .ok_or_else(|| format!("no array at `{path}`"))?
            .0;
        let elems = self.array_elem_spans(span);
        let e = *elems
            .get(i)
            .ok_or_else(|| format!("index {i} out of range"))?;
        self.remove_line_span(e.0, e.1)
    }

    // raw byte range replace. the caller owns the bytes. reparse validates
    pub fn replace(&mut self, start: usize, end: usize, replacement: &str) -> Result<(), String> {
        self.splice((start, end), replacement)
    }

    fn insert_line_before(
        &mut self,
        anchor: Span,
        raw: &str,
        pad: &str,
        comma: bool,
    ) -> Result<(), String> {
        let ls = line_start(&self.text, anchor.0);
        let on_own_line = self.text[ls..anchor.0].trim().is_empty();
        if on_own_line {
            let sep = if comma { "," } else { "" };
            self.splice((ls, ls), &format!("{pad}{raw}{sep}\n"))
        } else {
            let sep = if comma { ", " } else { " " };
            self.splice((anchor.0, anchor.0), &format!("{raw}{sep}"))
        }
    }

    fn append_top(&mut self, src: &str) -> Result<(), String> {
        let src = src.trim();
        if src.is_empty() {
            return Err("empty src".to_string());
        }
        let at = self.text.len();
        let mut add = String::new();
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            add.push('\n');
        }
        add.push('\n');
        add.push_str(src);
        add.push('\n');
        self.splice((at, at), &add)
    }

    fn insert_body(
        &mut self,
        open: Span,
        close_start: usize,
        depth: usize,
        raw: &str,
        inline_sep: &str,
    ) -> Result<(), String> {
        let unit = indent_unit(&self.text);
        let pad = unit.repeat(depth + 1);
        let close_pad = unit.repeat(depth);
        let (open_line, _) = crate::diag::line_col(&self.text, open.0);
        let (close_line, _) = crate::diag::line_col(&self.text, close_start);
        if open_line == close_line {
            let inner = &self.text[open.1..close_start];
            if inner.trim().is_empty() {
                let block = indent_block(raw, &pad);
                let new = format!("[\n{block}\n{close_pad}]");
                return self.splice((open.0, close_start + 1), &new);
            }
            // inline body. insert before the last non space byte
            let ins = open.1 + inner.trim_end().len();
            let sep = if inner.trim_end().ends_with(',') {
                " "
            } else {
                inline_sep
            };
            self.splice((ins, ins), &format!("{sep}{raw}"))
        } else {
            let block = indent_block(raw, &pad);
            let at = line_start(&self.text, close_start);
            let insert = format!("{block}\n");
            self.splice((at, at), &insert)
        }
    }

    fn insert_array(&mut self, span: Span, depth: usize, raw: &str) -> Result<(), String> {
        let unit = indent_unit(&self.text);
        // attr value lines sit one deeper than the attr key
        let pad = unit.repeat(depth + 2);
        let (open_line, _) = crate::diag::line_col(&self.text, span.0);
        let (close_line, _) = crate::diag::line_col(&self.text, span.1);
        let close_start = span.1.saturating_sub(1);
        if open_line == close_line {
            let inner = &self.text[span.0 + 1..close_start];
            if inner.trim().is_empty() {
                return self.splice((span.0, span.1), &format!("[{raw}]"));
            }
            let ins = span.0 + 1 + inner.trim_end().len();
            let sep = if inner.trim_end().ends_with(',') {
                " "
            } else {
                ", "
            };
            self.splice((ins, ins), &format!("{sep}{raw}"))
        } else {
            let block = indent_block(raw, &pad);
            let at = line_start(&self.text, close_start);
            let insert = format!("{block}\n");
            self.splice((at, at), &insert)
        }
    }

    fn remove_line_span(&mut self, a: usize, b: usize) -> Result<(), String> {
        let ls = line_start(&self.text, a);
        let before = self.text[ls..a].trim().is_empty();
        let le = self.text[b..]
            .find('\n')
            .map(|i| b + i)
            .unwrap_or(self.text.len());
        let after = self.text[b..le].trim().is_empty();
        if before && after {
            let end = if self.text[le..].starts_with('\n') {
                le + 1
            } else {
                le
            };
            self.splice((ls, end), "")
        } else {
            // inline. also eat one trailing comma and space
            let mut end = b;
            let rest = &self.text[b..];
            let trimmed = rest.trim_start();
            let skipped = rest.len() - trimmed.len();
            if trimmed.starts_with(',') {
                end = b + skipped + 1;
                if self.text[end..].starts_with(' ') {
                    end += 1;
                }
            }
            self.splice((a, end), "")
        }
    }

    fn array_span(&self, segs: &[&str]) -> Option<(Span, usize)> {
        let path = segs.join(".");
        if !matches!(self.get(&path), Some(Value::Array(_))) {
            return None;
        }
        let layout = top_layout(&self.text);
        let (name, rest) = segs.split_first()?;
        let key = rest.last()?;
        let descend = &rest[..rest.len().saturating_sub(1)];
        let node = find_node(&layout.nodes, name, descend)?;
        let attr = node.attrs.iter().find(|a| a.key == *key)?;
        Some((attr.value_span, node.depth))
    }

    fn node_body(&self, segs: &[&str]) -> Option<(Span, Span, usize, Option<usize>)> {
        let layout = top_layout(&self.text);
        let (name, rest) = segs.split_first()?;
        let node = find_node(&layout.nodes, name, rest)?;
        let open = node.open_span?;
        let close = node.close_span?;
        Some((open, close, node.depth, last_bare_child(node)))
    }

    // spans of every array element in order. elements are single tokens or
    // balanced bracket groups. commas are skipped so space separated arrays work
    fn array_elem_spans(&self, span: Span) -> Vec<Span> {
        let sub = &self.text[span.0..span.1];
        let lexed = lex::lex(sub, &Default::default(), Lang::En);
        let t = &lexed.tokens;
        let mut out = Vec::new();
        let mut i = if matches!(t.first().map(|x| &x.0), Some(Kind::LBracket)) {
            1
        } else {
            0
        };
        while i < t.len() {
            let (kind, off) = &t[i];
            match kind {
                Kind::RBracket => break,
                Kind::Comma => i += 1,
                Kind::LBracket => {
                    // balanced group
                    let mut d = 0i32;
                    let mut j = i;
                    loop {
                        match &t[j].0 {
                            Kind::LBracket => d += 1,
                            Kind::RBracket => {
                                d -= 1;
                                if d == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                        if j >= t.len() {
                            break;
                        }
                    }
                    out.push((span.0 + off, span.0 + token_end(sub, t[j].1, &t[j].0)));
                    i = j + 1;
                }
                _ => {
                    out.push((span.0 + off, span.0 + token_end(sub, *off, kind)));
                    i += 1;
                }
            }
        }
        out
    }

    // span of the array element whose text matches want
    fn array_elem_span(&self, span: Span, want: &str) -> Option<Span> {
        self.array_elem_spans(span)
            .into_iter()
            .find(|(a, b)| match_want(&self.text[*a..*b], want))
    }

    // span of a child node or attr named anchor inside the target node
    fn node_child_span(&self, segs: &[&str], anchor: &str) -> Option<Span> {
        let layout = top_layout(&self.text);
        let (name, rest) = segs.split_first()?;
        let node = find_node(&layout.nodes, name, rest)?;
        if let Some(c) = node.children.iter().find(|c| c.name == anchor) {
            return Some(c.full_span);
        }
        node.attrs
            .iter()
            .find(|a| a.key == anchor)
            .map(|a| a.full_span)
    }

    fn node_depth(&self, segs: &[&str]) -> Option<usize> {
        let layout = top_layout(&self.text);
        let (name, rest) = segs.split_first()?;
        find_node(&layout.nodes, name, rest).map(|n| n.depth)
    }

    // end of the bare sibling just before byte `before` inside the target node
    fn prev_bare_end(&self, segs: &[&str], before: usize) -> Option<usize> {
        let layout = top_layout(&self.text);
        let (name, rest) = segs.split_first()?;
        let node = find_node(&layout.nodes, name, rest)?;
        let mut best: Option<(usize, bool)> = None;
        for c in &node.children {
            let e = c.full_span.1;
            if e <= before && best.map(|(be, _)| e > be).unwrap_or(true) {
                best = Some((e, c.open_span.is_none()));
            }
        }
        for a in &node.attrs {
            let e = a.full_span.1;
            if e <= before && best.map(|(be, _)| e > be).unwrap_or(true) {
                best = Some((e, false));
            }
        }
        match best {
            Some((end, true)) => Some(end),
            _ => None,
        }
    }

    // first statement line in a func body whose leading token equals stmt
    fn func_stmt_span(&self, func: &str, stmt: &str) -> Option<Span> {
        let layout = top_layout(&self.text);
        let f = layout.funcs.iter().find(|f| f.name == func)?;
        let base = f.open_span.1;
        let body = &self.text[base..f.close_start];
        let mut off = 0;
        for line in body.split_inclusive('\n') {
            let trimmed = line.trim();
            if !trimmed.is_empty() && first_token(trimmed) == stmt {
                let lead = line.len() - line.trim_start().len();
                let start = base + off + lead;
                let end = start + trimmed.len();
                return Some((start, end));
            }
            off += line.len();
        }
        None
    }

    fn find_attr(&self, segs: &[&str]) -> Option<AttrLayout> {
        let layout = top_layout(&self.text);
        let (name, rest) = segs.split_first()?;
        let key = rest.last()?;
        let descend = &rest[..rest.len().saturating_sub(1)];
        let node = find_node(&layout.nodes, name, descend)?;
        node.attrs.iter().find(|a| a.key == *key).cloned()
    }

    // run a batch of edits. any error rolls the whole batch back
    pub fn transaction<F>(&mut self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut DocFile) -> Result<(), String>,
    {
        let old_text = self.text.clone();
        let old_doc = self.doc.clone();
        match f(self) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.text = old_text;
                self.doc = old_doc;
                Err(e)
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let p = self.path.as_ref().ok_or("no path. use save_as")?;
        std::fs::write(p, &self.text).map_err(|e| format!("write {} failed: {e}", p.display()))
    }

    pub fn save_as(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let p = path.as_ref();
        std::fs::write(p, &self.text).map_err(|e| format!("write {} failed: {e}", p.display()))
    }

    fn splice(&mut self, span: Span, replacement: &str) -> Result<(), String> {
        let (a, b) = span;
        let (a, b) = (a.min(self.text.len()), b.min(self.text.len()));
        if a > b || !self.text.is_char_boundary(a) || !self.text.is_char_boundary(b) {
            return Err("edit landed on a bad boundary".to_string());
        }
        let old_text = self.text.clone();
        let old_doc = self.doc.clone();
        self.text.replace_range(a..b, replacement);
        let out = parse_with(
            &self.text,
            &ParseOptions {
                vars: Default::default(),
                lang: Lang::En,
                fix: false,
                keep_interp: true,
            },
        );
        match out.doc {
            Some(doc) => {
                self.doc = doc;
                Ok(())
            }
            None => {
                // put the old text back. splice is all or nothing
                self.text = old_text;
                self.doc = old_doc;
                Err("edit produced a bad doc. rolled back".to_string())
            }
        }
    }

    fn attr_value_span(&self, segs: &[&str]) -> Option<Span> {
        // func.desc path
        if segs.len() == 2 && segs[1] == "desc" {
            let flats = top_layout(&self.text);
            if let Some(f) = flats.funcs.iter().find(|f| f.name == segs[0]) {
                if let Some(s) = find_desc_span(&self.text, f.open_start) {
                    return Some(s);
                }
            }
        }
        let layout = top_layout(&self.text);
        let (name, rest) = segs.split_first()?;
        // last seg is the attr key the rest are child nodes
        let key = rest.last()?;
        let descend = &rest[..rest.len().saturating_sub(1)];
        let node = find_node(&layout.nodes, name, descend)?;
        node.attrs
            .iter()
            .find(|a| a.key == *key)
            .map(|a| a.value_span)
    }
}

fn lookup_in_node<'a>(node: &'a Node, segs: &[&str]) -> Option<&'a Value> {
    let (head, rest) = segs.split_first()?;
    if rest.is_empty() {
        return node.get(head);
    }
    if let Some(v) = node.get(head) {
        return lookup_in_value(v, rest);
    }
    let child = node.get_node(head)?;
    lookup_in_node(child, rest)
}

fn lookup_in_value<'a>(v: &'a Value, segs: &[&str]) -> Option<&'a Value> {
    let (head, rest) = segs.split_first()?;
    match v {
        Value::Map(_) => {
            let next = v.map_get(head)?;
            lookup_in_value(next, rest)
        }
        Value::Array(items) => {
            let i: usize = head.parse().ok()?;
            lookup_in_value(items.get(i)?, rest)
        }
        _ => None,
    }
}

// layout tree byte spans only no ast
struct NodeLayout {
    name: String,
    depth: usize,
    full_span: Span,
    open_span: Option<Span>,
    close_span: Option<Span>,
    attrs: Vec<AttrLayout>,
    children: Vec<NodeLayout>,
}

#[derive(Clone)]
struct AttrLayout {
    key: String,
    value_span: Span,
    full_span: Span,
}

struct FuncLayout {
    name: String,
    depth: usize,
    // byte start of the body `[`
    open_start: usize,
    open_span: Span,
    close_start: usize,
    // whole `fun ... [ ... ]`
    full_span: Span,
}

struct Layout {
    nodes: Vec<NodeLayout>,
    funcs: Vec<FuncLayout>,
}

struct Builder<'a> {
    t: &'a [(Kind, usize)],
    src: &'a str,
}

impl<'a> Builder<'a> {
    fn kind(&self, i: usize) -> Option<&Kind> {
        self.t.get(i).map(|x| &x.0)
    }

    fn start(&self, i: usize) -> usize {
        self.t.get(i).map(|x| x.1).unwrap_or(self.src.len())
    }

    fn skip_commas(&self, i: &mut usize) {
        while matches!(self.kind(*i), Some(Kind::Comma)) {
            *i += 1;
        }
    }

    fn is_attr(&self, i: usize) -> bool {
        matches!(self.kind(i), Some(Kind::Name(_)))
            && matches!(self.kind(i + 1), Some(Kind::Colon) | Some(Kind::Equals))
    }

    fn value_end(&self, i: usize) -> usize {
        if matches!(self.kind(i), Some(Kind::LBracket)) {
            let mut depth = 0i32;
            let mut j = i;
            while j < self.t.len() {
                match self.kind(j) {
                    Some(Kind::LBracket) => depth += 1,
                    Some(Kind::RBracket) => {
                        depth -= 1;
                        if depth == 0 {
                            return self.tok_end(j);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            return self.src.len();
        }
        self.tok_end(i)
    }

    fn tok_end(&self, i: usize) -> usize {
        match self.t.get(i) {
            Some((k, s)) => token_end(self.src, *s, k),
            None => self.src.len(),
        }
    }

    fn parse_attr(&self, i: &mut usize) -> AttrLayout {
        let key = match self.kind(*i) {
            Some(Kind::Name(s)) => s.clone(),
            _ => String::new(),
        };
        let key_start = self.start(*i);
        *i += 1; // key
        *i += 1; // separator
        let vstart = *i;
        let vend = self.value_end(*i);
        // advance past every token inside the value
        while *i < self.t.len() && self.start(*i) < vend {
            *i += 1;
        }
        self.skip_commas(i);
        AttrLayout {
            key,
            value_span: (self.start(vstart), vend),
            full_span: (key_start, vend),
        }
    }

    fn parse_node(&self, i: &mut usize, depth: usize) -> NodeLayout {
        let name = match self.kind(*i) {
            Some(Kind::Name(s)) => s.clone(),
            _ => String::new(),
        };
        let start = self.start(*i);
        *i += 1;
        // scalar args until a body or attr key
        let mut end = start + name.len();
        while *i < self.t.len() {
            if self.is_attr(*i) {
                break;
            }
            match self.kind(*i) {
                Some(Kind::LBracket) | Some(Kind::RBracket) | Some(Kind::Comma) => break,
                Some(Kind::Name(_))
                | Some(Kind::Str(_))
                | Some(Kind::RawStr(_))
                | Some(Kind::Int(_))
                | Some(Kind::Float(_))
                | Some(Kind::Bool(_))
                | Some(Kind::Null) => {
                    end = self.value_end(*i);
                    *i += 1;
                }
                _ => break,
            }
        }
        let mut attrs = Vec::new();
        let mut children = Vec::new();
        let mut open_span = None;
        let mut close_span = None;
        let mut full_end = end;
        if matches!(self.kind(*i), Some(Kind::LBracket)) {
            open_span = Some((self.start(*i), self.tok_end(*i)));
            *i += 1;
            loop {
                self.skip_commas(i);
                match self.kind(*i) {
                    None => break,
                    Some(Kind::RBracket) => {
                        close_span = Some((self.start(*i), self.tok_end(*i)));
                        full_end = self.tok_end(*i);
                        *i += 1;
                        break;
                    }
                    Some(Kind::Name(_)) if self.is_attr(*i) => {
                        attrs.push(self.parse_attr(i));
                    }
                    Some(Kind::Name(_)) => {
                        children.push(self.parse_node(i, depth + 1));
                    }
                    _ => {
                        *i += 1;
                    }
                }
            }
        }
        NodeLayout {
            name,
            depth,
            full_span: (start, full_end),
            open_span,
            close_span,
            attrs,
            children,
        }
    }

    fn parse_func(&self, i: &mut usize, depth: usize) -> FuncLayout {
        let fun_start = self.start(*i);
        let name = match self.kind(*i + 1) {
            Some(Kind::Name(s)) => s.clone(),
            _ => String::new(),
        };
        *i += 2; // fun name
        // params skip balanced parens
        if matches!(self.kind(*i), Some(Kind::LParen)) {
            let mut d = 0i32;
            while *i < self.t.len() {
                match self.kind(*i) {
                    Some(Kind::LParen) => d += 1,
                    Some(Kind::RParen) => {
                        d -= 1;
                        if d == 0 {
                            *i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                *i += 1;
            }
        }
        let open_start = self.start(*i);
        let mut open_span = (open_start, open_start);
        let mut close_start = open_start;
        let mut fun_end = open_start;
        if matches!(self.kind(*i), Some(Kind::LBracket)) {
            open_span = (open_start, self.tok_end(*i));
            let mut d = 0i32;
            while *i < self.t.len() {
                match self.kind(*i) {
                    Some(Kind::LBracket) => d += 1,
                    Some(Kind::RBracket) => {
                        d -= 1;
                        if d == 0 {
                            close_start = self.start(*i);
                            fun_end = self.tok_end(*i);
                            *i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                *i += 1;
            }
        }
        FuncLayout {
            name,
            depth,
            open_start,
            open_span,
            close_start,
            full_span: (fun_start, fun_end),
        }
    }
}

fn top_layout(src: &str) -> Layout {
    let lexed = lex::lex(src, &Default::default(), Lang::En);
    let b = Builder {
        t: &lexed.tokens,
        src,
    };
    let mut nodes = Vec::new();
    let mut funcs = Vec::new();
    let mut i = 0;
    while i < b.t.len() {
        b.skip_commas(&mut i);
        match b.kind(i) {
            None => break,
            Some(Kind::Fun) => funcs.push(b.parse_func(&mut i, 0)),
            Some(Kind::Name(_)) => nodes.push(b.parse_node(&mut i, 0)),
            Some(Kind::Impt) => {
                i += 1;
                while matches!(
                    b.kind(i),
                    Some(Kind::Str(_)) | Some(Kind::RawStr(_)) | Some(Kind::Comma)
                ) {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    Layout { nodes, funcs }
}

fn find_node<'a>(nodes: &'a [NodeLayout], name: &str, rest: &[&str]) -> Option<&'a NodeLayout> {
    let mut cur = nodes.iter().find(|n| n.name == name)?;
    for seg in rest {
        if *seg == "desc" {
            return None;
        }
        cur = cur.children.iter().find(|n| n.name == *seg)?;
    }
    Some(cur)
}

fn find_desc_span(src: &str, open_start: usize) -> Option<Span> {
    let lexed = lex::lex(src, &Default::default(), Lang::En);
    let t = &lexed.tokens;
    // locate the body `[` by its byte start scan from the next token
    let mut i = t
        .iter()
        .position(|(k, s)| matches!(k, Kind::LBracket) && *s == open_start)?
        + 1;
    let mut depth = 0i32;
    while i < t.len() {
        match &t[i].0 {
            Kind::LBracket => depth += 1,
            Kind::RBracket => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            Kind::Name(s) if s == "desc" && depth == 0 => {
                let sep = t.get(i + 1).map(|x| &x.0);
                if matches!(sep, Some(Kind::Colon) | Some(Kind::Equals)) {
                    let vi = i + 2;
                    if let Some((vk, vs)) = t.get(vi) {
                        if matches!(vk, Kind::Str(_) | Kind::RawStr(_)) {
                            return Some((*vs, token_end(src, *vs, vk)));
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// the last child or attr in a body. Some(pos) when it is a bare node that
// would swallow the next bare node on a newline
fn last_bare_child(n: &NodeLayout) -> Option<usize> {
    let mut best: Option<(usize, bool)> = None;
    for c in &n.children {
        let e = c.full_span.1;
        let bare = c.open_span.is_none();
        if best.map(|(be, _)| e > be).unwrap_or(true) {
            best = Some((e, bare));
        }
    }
    for a in &n.attrs {
        let e = a.full_span.1;
        if best.map(|(be, _)| e > be).unwrap_or(true) {
            best = Some((e, false));
        }
    }
    match best {
        Some((end, true)) => Some(end),
        _ => None,
    }
}

// compare an array element text to a wanted name. strips quotes
fn match_want(txt: &str, want: &str) -> bool {
    let t = txt.trim();
    if t == want {
        return true;
    }
    let inner = if t.starts_with("#\"") && t.ends_with("\"#") && t.len() >= 4 {
        &t[2..t.len() - 2]
    } else if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        &t[1..t.len() - 1]
    } else {
        t
    };
    inner == want
}

// leading token of a statement line
fn first_token(s: &str) -> &str {
    s.split(|c: char| c.is_whitespace() || matches!(c, '[' | '(' | ',' | ':' | '='))
        .next()
        .unwrap_or("")
}

// indent every non empty line of a multi line snippet
fn indent_block(raw: &str, pad: &str) -> String {
    raw.lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("{pad}{}", l.trim_end())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// match the file. tabs win if they dominate else two spaces
fn indent_unit(text: &str) -> String {
    let mut tabs = 0usize;
    let mut spaces = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let lead: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if lead.contains('\t') {
            tabs += 1;
        } else if !lead.is_empty() {
            spaces += 1;
        }
    }
    if tabs > spaces {
        "\t".to_string()
    } else {
        "  ".to_string()
    }
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at.min(src.len())]
        .rfind('\n')
        .map(|p| p + 1)
        .unwrap_or(0)
}

fn token_end(src: &str, start: usize, kind: &Kind) -> usize {
    match kind {
        Kind::Str(_) => scan_string_end(src, start),
        Kind::RawStr(_) => scan_raw_end(src, start),
        Kind::Name(s) => start + s.len(),
        Kind::Int(_) | Kind::Float(_) => scan_run(src, start, |c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-')
        }),
        Kind::Bool(true) => start + 4,
        Kind::Bool(false) => start + 5,
        Kind::Null => start + 4,
        Kind::Impt => start + 4,
        Kind::Fun => start + 3,
        Kind::Let => start + 3,
        Kind::If => start + 2,
        Kind::Else => start + 4,
        Kind::While => start + 5,
        Kind::For => start + 3,
        Kind::In => start + 2,
        Kind::Return => start + 6,
        Kind::EqEq | Kind::Ne | Kind::Le | Kind::Ge | Kind::AmpAmp | Kind::PipePipe => start + 2,
        _ => start + 1,
    }
}

fn scan_run(src: &str, start: usize, ok: impl Fn(char) -> bool) -> usize {
    let mut i = start;
    for (off, c) in src[start..].char_indices() {
        if ok(c) {
            i = start + off + c.len_utf8();
        } else {
            break;
        }
    }
    i
}

fn scan_string_end(src: &str, start: usize) -> usize {
    let bytes = src.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            b'\n' => return i,
            _ => i += 1,
        }
    }
    bytes.len()
}

fn scan_raw_end(src: &str, start: usize) -> usize {
    let bytes = src.as_bytes();
    let mut i = start + 2;
    while i + 1 < bytes.len() {
        if bytes[i] == b'"' && bytes[i + 1] == b'#' {
            return i + 2;
        }
        i += 1;
    }
    bytes.len()
}
