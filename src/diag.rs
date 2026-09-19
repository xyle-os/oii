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
        std::env::var("OII_LANG")
            .ok()
            .and_then(|s| Lang::parse(&s))
            .unwrap_or(Lang::Zh)
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
            level_word: tr(lang, "警告", "warning"),
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
    Loc {
        line,
        col,
        end_line,
        end_col,
    }
}

pub fn render_diag(src: &str, d: &Diag) -> String {
    render_diag_with_file(src, None, d)
}

// file shows in header. pass None for stdin pipes.
pub fn render_diag_with_file(src: &str, file: Option<&str>, d: &Diag) -> String {
    let mut out = String::new();
    // eof errors point past the last line. clamp to eol.
    let total = src.lines().count().max(1) as u32;
    let over = d.loc.line > total;
    let line_no = d.loc.line.min(total);
    let last_len = src
        .lines()
        .nth(line_no.saturating_sub(1) as usize)
        .map(|l| l.chars().count() as u32)
        .unwrap_or(0);
    let (col, end_col, end_line) = if over {
        (last_len + 1, last_len + 1, line_no)
    } else {
        (d.loc.col, d.loc.end_col, d.loc.end_line)
    };
    let pos = if line_no == end_line {
        format!("{line_no}:{col}")
    } else {
        format!("{line_no}:{col}-{end_line}:{end_col}")
    };
    match file {
        Some(f) => out.push_str(&format!("{f}:{pos}: ")),
        None => out.push_str(&format!("{pos}: ")),
    }
    out.push_str(&format!("{} [{}] {}\n", d.level_word, d.code, d.message));
    if let Some(line_text) = src.lines().nth(line_no.saturating_sub(1) as usize) {
        let shown = expand_tabs(line_text);
        // reuse span math with clamped loc
        let tmp = Diag {
            level: d.level,
            level_word: d.level_word,
            code: d.code,
            message: String::new(),
            hint: None,
            loc: Loc {
                line: line_no,
                col,
                end_line,
                end_col,
            },
        };
        let (lead, len) = caret_span(line_text, &tmp);
        out.push_str(&format!(
            "   {line_no} | {shown}\n   {line_no} | {lead}{len}\n"
        ));
    }
    if let Some(h) = &d.hint {
        // tag stays english. grep-friendly.
        out.push_str(&format!("   hint: {h}\n"));
    }
    out
}

// tabs expand to 4-stop. caret must match shown text.
fn expand_tabs(s: &str) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    for ch in s.chars() {
        if ch == '\t' {
            let n = 4 - col % 4;
            for _ in 0..n {
                out.push(' ');
            }
            col += n;
        } else {
            out.push(ch);
            col += char_width(ch);
        }
    }
    out
}

// display column of char col (1-based) in expanded line
fn display_col(line: &str, col: u32) -> usize {
    let mut disp = 0usize;
    let mut cur = 1u32;
    for ch in line.chars() {
        if cur >= col {
            break;
        }
        disp += if ch == '\t' {
            4 - disp % 4
        } else {
            char_width(ch)
        };
        cur += 1;
    }
    disp
}

// (leading spaces, caret len) in display cells
fn caret_span(line: &str, d: &Diag) -> (String, String) {
    let start = display_col(line, d.loc.col);
    let chars: Vec<char> = line.chars().collect();
    let end_col = d.loc.end_col.max(d.loc.col + 1);
    let same_line = d.loc.line == d.loc.end_line;
    // cover source chars in span, at least one cell
    let mut len = 0usize;
    if same_line {
        let from = (d.loc.col.saturating_sub(1) as usize).min(chars.len());
        let to = (end_col.saturating_sub(1) as usize).min(chars.len());
        for (idx, ch) in chars.iter().enumerate() {
            if idx >= from && idx < to {
                len += if *ch == '\t' {
                    1
                } else {
                    char_width(*ch).max(1)
                };
            }
        }
        // span ran past eol (eof case). flag one cell.
        if to >= chars.len() && from >= chars.len() {
            len = 1;
        }
        len = len.max(1);
    } else {
        // multiline. mark to eol.
        len = 1;
    }
    let mark = if d.level == Level::Error { '^' } else { '-' };
    (
        " ".repeat(start),
        std::iter::repeat(mark).take(len).collect(),
    )
}

// east asian wide chars take two cells
fn char_width(ch: char) -> usize {
    let u = ch as u32;
    match u {
        0x1100..=0x115F
        | 0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFFEF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}
