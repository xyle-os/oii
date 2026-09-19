use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Option<Lang> {
        match s {
            "zh" | "zh-CN" | "zh-cn" | "cn" => Some(Lang::Zh),
            "en" | "en-US" | "en-us" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn from_env() -> Lang {
        std::env::var("OII_LANG").ok().and_then(|s| Lang::parse(&s)).unwrap_or(Lang::Zh)
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lang::Zh => write!(f, "zh"),
            Lang::En => write!(f, "en"),
        }
    }
}

impl std::str::FromStr for Lang {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Lang::parse(s).ok_or_else(|| format!("unknown language: {s}. want zh or en"))
    }
}

pub fn tr<'a>(lang: Lang, zh: &'a str, en: &'a str) -> &'a str {
    match lang {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    Fix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loc {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

#[derive(Debug, Clone)]
pub struct Diag {
    pub level: Level,
    pub level_word: &'static str,
    pub code: &'static str,
    pub message: String,
    pub hint: Option<String>,
    pub loc: Loc,
}

impl Diag {
    pub fn error(lang: Lang, code: &'static str, zh: &str, en: &str, loc: Loc) -> Diag {
        Diag::error_hint(lang, code, zh, en, None, None, loc)
    }

    pub fn error_hint(
        lang: Lang,
        code: &'static str,
        zh: &str,
        en: &str,
        zh_hint: Option<&str>,
        en_hint: Option<&str>,
        loc: Loc,
    ) -> Diag {
        Diag {
            level: Level::Error,
            level_word: tr(lang, "错误", "error"),
            code,
            message: tr(lang, zh, en).to_string(),
            hint: match (zh_hint, en_hint) {
                (Some(a), Some(b)) => Some(tr(lang, a, b).to_string()),
                (Some(a), None) => Some(a.to_string()),
                (None, Some(b)) => Some(b.to_string()),
                _ => None,
            },
            loc,
        }
    }

    pub fn warning(lang: Lang, code: &'static str, zh: &str, en: &str, loc: Loc) -> Diag {
        Diag::warning_hint(lang, code, zh, en, None, None, loc)
    }

    pub fn warning_hint(
        lang: Lang,
        code: &'static str,
        zh: &str,
        en: &str,
        zh_hint: Option<&str>,
        en_hint: Option<&str>,
        loc: Loc,
    ) -> Diag {
        Diag {
            level: Level::Warning,
            level_word: tr(lang, "警告", "warn"),
            code,
            message: tr(lang, zh, en).to_string(),
            hint: match (zh_hint, en_hint) {
                (Some(a), Some(b)) => Some(tr(lang, a, b).to_string()),
                (Some(a), None) => Some(a.to_string()),
                (None, Some(b)) => Some(b.to_string()),
                _ => None,
            },
            loc,
        }
    }

    pub fn fix(lang: Lang, code: &'static str, zh: &str, en: &str, loc: Loc) -> Diag {
        Diag::fix_hint(lang, code, zh, en, None, None, loc)
    }

    pub fn fix_hint(
        lang: Lang,
        code: &'static str,
        zh: &str,
        en: &str,
        zh_hint: Option<&str>,
        en_hint: Option<&str>,
        loc: Loc,
    ) -> Diag {
        Diag {
            level: Level::Fix,
            level_word: tr(lang, "修复", "fix"),
            code,
            message: tr(lang, zh, en).to_string(),
            hint: match (zh_hint, en_hint) {
                (Some(a), Some(b)) => Some(tr(lang, a, b).to_string()),
                (Some(a), None) => Some(a.to_string()),
                (None, Some(b)) => Some(b.to_string()),
                _ => None,
            },
            loc,
        }
    }
}

pub fn line_col(src: &str, off: usize) -> (u32, u32) {
    let off = off.min(src.len());
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, ch) in src.char_indices() {
        if i >= off {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn loc_of(src: &str, a: usize, b: usize) -> Loc {
    let a = a.min(src.len());
    let b = b.max(a).min(src.len());
    let (line, col) = line_col(src, a);
    let (end_line, end_col) = line_col(src, b);
    Loc { line, col, end_line, end_col }
}

pub fn render_diag(src: &str, d: &Diag) -> String {
    let mut out = String::new();
    let pos = format!("{}:{}", d.loc.line, d.loc.col);
    out.push_str(&format!("{} {} [{}] {}\n", d.level_word, pos, d.code, d.message));
    if let Some(line_text) = src.lines().nth(d.loc.line.saturating_sub(1) as usize) {
        let caret_len = if d.loc.line == d.loc.end_line {
            (d.loc.end_col - d.loc.col).max(1) as usize
        } else {
            1
        };
        let mut caret = " ".repeat(d.loc.col.saturating_sub(1) as usize);
        for _ in 0..caret_len {
            caret.push(if d.level == Level::Error { '^' } else { '-' });
        }
        out.push_str(&format!("   {} | {}\n   {} | {}\n", d.loc.line, line_text, d.loc.line, caret));
    }
    if let Some(h) = &d.hint {
        out.push_str(&format!("   hint: {h}\n"));
    }
    out
}
