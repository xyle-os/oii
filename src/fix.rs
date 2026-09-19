use crate::diag::{Diag, Lang, loc_of};

pub fn apply_auto_fix(src: &str, lang: Lang) -> (String, Vec<Diag>) {
    let mut open = 0usize;
    let mut close = 0usize;
    let mut chars = src.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                while let Some(d) = chars.next() {
                    if d == '\\' {
                        if chars.next().is_some() {}
                        continue;
                    }
                    if d == '"' || d == '\n' {
                        break;
                    }
                }
            }
            '#' if chars.peek() == Some(&'"') => {
                chars.next();
                let mut prev = '\0';
                loop {
                    match chars.next() {
                        Some(d) => {
                            if d == '#' && prev == '"' {
                                break;
                            }
                            prev = d;
                        }
                        None => break,
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                while let Some(d) = chars.next() {
                    if d == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                loop {
                    match chars.next() {
                        Some(d) => {
                            if d == '/' && prev == '*' {
                                break;
                            }
                            prev = d;
                        }
                        None => break,
                    }
                }
            }
            '[' => {
                open += 1;
            }
            ']' => {
                close += 1;
            }
            _ => {}
        }
    }

    let deficit = open.saturating_sub(close);
    let mut diags = Vec::new();
    if deficit > 0 {
        diags.push(Diag::fix_hint(
            lang,
            "F001",
            format!("结尾缺 {deficit} 个 `]`. 补上了").as_str(),
            format!("missing {deficit} `]` at end. appended").as_str(),
            Some("每个 `[` 要配一个 `]`"),
            Some("each `[` needs a `]`"),
            loc_of(src, src.len(), src.len()),
        ));
    }

    let mut fixed = src.to_string();
    for _ in 0..deficit {
        fixed.push_str("\n]");
    }
    (fixed, diags)
}
