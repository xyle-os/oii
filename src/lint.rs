use std::collections::{HashMap, HashSet};

use crate::diag::{Diag, Lang, loc_of};
use crate::lex::Kind;

// token-level lints. run only on docs that parsed clean.
pub fn check(tokens: &[(Kind, usize)], src: &str, lang: Lang) -> Vec<Diag> {
    let mut out = Vec::new();
    dup_keys(tokens, src, lang, &mut out);
    swallowed_line(tokens, src, lang, &mut out);
    out
}

// last wins is the rule. say so loudly.
fn dup_keys(tokens: &[(Kind, usize)], src: &str, lang: Lang, out: &mut Vec<Diag>) {
    // one key set per open bracket
    let mut stack: Vec<HashSet<String>> = vec![HashSet::new()];
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i].0 {
            Kind::LBracket => {
                stack.push(HashSet::new());
                i += 1;
            }
            Kind::RBracket => {
                stack.pop();
                if stack.is_empty() {
                    stack.push(HashSet::new());
                }
                i += 1;
            }
            Kind::Name(key) => {
                let is_attr = matches!(
                    tokens.get(i + 1).map(|t| &t.0),
                    Some(Kind::Colon) | Some(Kind::Equals)
                );
                if is_attr {
                    let byte = tokens[i].1;
                    if !stack.last_mut().unwrap().insert(key.clone()) {
                        out.push(Diag::warning_hint(
                            lang,
                            "W002",
                            format!("属性 `{key}` 重复了. 留最后一个").as_str(),
                            "duplicate key. last one wins",
                            Some("删掉旧的，只留一个"),
                            Some("keep one, drop the rest"),
                            loc_of(src, byte, byte + key.len()),
                        ));
                    }
                    i += 2; // skip sep, value handled next rounds
                } else {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }
}

// `foo` then `bar` on next line is one node with arg. often a typo.
fn swallowed_line(tokens: &[(Kind, usize)], src: &str, lang: Lang, out: &mut Vec<Diag>) {
    // map byte offset to line
    let mut line_of: HashMap<usize, u32> = HashMap::new();
    let mut line = 1u32;
    for (idx, ch) in src.char_indices() {
        line_of.insert(idx, line);
        if ch == '\n' {
            line += 1;
        }
    }
    let at_line = |byte: usize| -> u32 {
        if byte >= src.len() {
            return line;
        }
        *line_of.get(&byte).unwrap_or(&line)
    };
    let mut depth = 0usize;
    let mut i = 0;
    while i + 1 < tokens.len() {
        match &tokens[i].0 {
            Kind::LBracket => depth += 1,
            Kind::RBracket => depth = depth.saturating_sub(1),
            _ => {}
        }
        // top level, two bare words back to back on different lines
        if depth == 0 {
            if let (Kind::Name(_), Kind::Name(next)) = (&tokens[i].0, &tokens[i + 1].0) {
                let a = tokens[i].1;
                let b = tokens[i + 1].1;
                if at_line(a) != at_line(b) {
                    out.push(Diag::warning_hint(
                        lang,
                        "W003",
                        "换行不断节点. `bar` 成了上一行的参数",
                        "newline does not end a node",
                        Some("本意换行就给上一行加 []"),
                        Some("add [] to end the node"),
                        loc_of(src, b, b + next.len()),
                    ));
                }
            }
        }
        i += 1;
    }
}
