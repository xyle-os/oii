use std::collections::HashMap;

use crate::diag::{Diag, Lang, loc_of, tr};
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Name(String),
    Str(String),
    RawStr(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Impt,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Equals,
}

impl std::hash::Hash for Kind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Kind::Name(s) => s.hash(state),
            Kind::Str(s) => s.hash(state),
            Kind::RawStr(s) => s.hash(state),
            Kind::Int(i) => i.hash(state),
            Kind::Float(x) => x.to_bits().hash(state),
            Kind::Bool(b) => b.hash(state),
            _ => {}
        }
    }
}

impl Eq for Kind {}

impl Kind {
    pub fn describe(&self, lang: Lang) -> String {
        let with = |zh: &str, en: &str| tr(lang, zh, en).to_string();
        match self {
            Kind::Name(s) => format!("{} `{s}`", with("名", "name")),
            Kind::Str(_) => with("字符串", "string"),
            Kind::RawStr(_) => with("原始串", "raw"),
            Kind::Int(_) => with("整数", "int"),
            Kind::Float(_) => with("浮点", "float"),
            Kind::Bool(_) => with("布尔", "bool"),
            Kind::Null => "null".to_string(),
            Kind::Impt => "impt".to_string(),
            Kind::LBracket => "`[`".to_string(),
            Kind::RBracket => "`]`".to_string(),
            Kind::LBrace => "`{`".to_string(),
            Kind::RBrace => "`}`".to_string(),
            Kind::Comma => "`,`".to_string(),
            Kind::Colon => "`:`".to_string(),
            Kind::Equals => "`=`".to_string(),
        }
    }
}

pub struct LexOut {
    pub tokens: Vec<(Kind, usize)>,
    pub diags: Vec<Diag>,
}

struct Lexer<'a> {
    src: &'a str,
    vars: &'a HashMap<String, String>,
    lang: Lang,
    keep_interp: bool,
    chars: Vec<char>,
    pos: usize,
    byte: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str, vars: &'a HashMap<String, String>, lang: Lang) -> Self {
        Lexer {
            src,
            vars,
            lang,
            keep_interp: false,
            chars: src.chars().collect(),
            pos: 0,
            byte: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if let Some(c) = c {
            self.pos += 1;
            self.byte += c.len_utf8();
        }
        c
    }

    fn err(&self, code: &'static str, zh: &str, en: &str, a: usize, b: usize) -> Diag {
        Diag::error(self.lang, code, zh, en, loc_of(self.src, a, b))
    }

    fn err_hint(
        &self,
        code: &'static str,
        zh: &str,
        en: &str,
        zh_hint: &'static str,
        en_hint: &'static str,
        a: usize,
        b: usize,
    ) -> Diag {
        Diag::error_hint(
            self.lang,
            code,
            zh,
            en,
            Some(zh_hint),
            Some(en_hint),
            loc_of(self.src, a, b),
        )
    }
}

pub fn lex(src: &str, vars: &HashMap<String, String>, lang: Lang) -> LexOut {
    lex_with(src, vars, lang, false)
}

pub fn lex_with(
    src: &str,
    vars: &HashMap<String, String>,
    lang: Lang,
    keep_interp: bool,
) -> LexOut {
    let mut lx = Lexer::new(src, vars, lang);
    lx.keep_interp = keep_interp;
    let mut tokens = Vec::new();
    let mut diags = Vec::new();

    loop {
        let before = lx.pos;
        let c = match lx.peek() {
            Some(c) => c,
            None => break,
        };
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                lx.bump();
            }
            '/' => match lx.peek2() {
                Some('/') => {
                    while let Some(c) = lx.peek() {
                        if c == '\n' {
                            break;
                        }
                        lx.bump();
                    }
                }
                Some('*') => {
                    let start = lx.byte;
                    lx.bump();
                    lx.bump();
                    let mut closed = false;
                    while lx.peek().is_some() {
                        if lx.peek() == Some('*') && lx.peek2() == Some('/') {
                            lx.bump();
                            lx.bump();
                            closed = true;
                            break;
                        }
                        lx.bump();
                    }
                    if !closed {
                        diags.push(lx.err(
                            "E008",
                            "块注释没闭合",
                            "unclosed block comment",
                            start,
                            lx.byte,
                        ));
                    }
                }
                _ => {
                    let start = lx.byte;
                    lx.bump();
                    diags.push(lx.err_hint(
                        "E011",
                        "字符 `/` 非法. 注释只有 // 或 /*",
                        "bad `/`. comments are // or /* only",
                        "只认 // 和 /*",
                        "use // or /*",
                        start,
                        lx.byte,
                    ));
                }
            },
            '[' => {
                tokens.push((Kind::LBracket, lx.byte));
                lx.bump();
            }
            ']' => {
                tokens.push((Kind::RBracket, lx.byte));
                lx.bump();
            }
            '{' | '}' => {
                let kind = if c == '{' { Kind::LBrace } else { Kind::RBrace };
                let start = lx.byte;
                tokens.push((kind, start));
                lx.bump();
                diags.push(lx.err_hint(
                    "E009",
                    "花括号不划作用域. 只认方括号",
                    "braces do not scope. use brackets",
                    "作用域写 名字 [ ... ]",
                    "scope is name [ ... ]",
                    start,
                    lx.byte,
                ));
            }
            ',' => {
                tokens.push((Kind::Comma, lx.byte));
                lx.bump();
            }
            ':' => {
                tokens.push((Kind::Colon, lx.byte));
                lx.bump();
            }
            '=' => {
                tokens.push((Kind::Equals, lx.byte));
                lx.bump();
            }
            '"' => {
                scan_string(&mut lx, &mut tokens, &mut diags);
            }
            '#' => {
                let start = lx.byte;
                if lx.peek2() == Some('"') {
                    scan_raw_string(&mut lx, &mut tokens, &mut diags);
                } else {
                    lx.bump();
                    diags.push(lx.err(
                        "E011",
                        "字符 `#` 非法. 原始字符串是 #\" 开头 \"# 结尾",
                        "bad `#`. raw string is #\" to \"#",
                        start,
                        lx.byte,
                    ));                }
            }
            '0'..='9' | '.' => {
                if !scan_number(&mut lx, &mut tokens, &mut diags) {
                    scan_word(&mut lx, &mut tokens);
                }
            }
            '-' | '+' => {
                let next = lx.peek2();
                let is_num = next
                    .map(|d| d.is_ascii_digit() || d == '.')
                    .unwrap_or(false);
                if is_num {
                    if !scan_number(&mut lx, &mut tokens, &mut diags) {
                        scan_word(&mut lx, &mut tokens);
                    }
                } else if next
                    .map(|d| d.is_alphanumeric() || d == '_' || d == '.')
                    .unwrap_or(false)
                {
                    scan_word(&mut lx, &mut tokens);
                } else {
                    let start = lx.byte;
                    let bad = lx.peek().unwrap_or('?');
                    lx.bump();
                    diags.push(lx.err(
                        "E011",
                        format!("非法字符 `{bad}`").as_str(),
                        format!("bad char `{bad}`").as_str(),
                        start,
                        lx.byte,
                    ));
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                scan_word(&mut lx, &mut tokens);
            }
            _ => {
                let start = lx.byte;
                lx.bump();
                diags.push(lx.err(
                    "E011",
                    format!("非法字符 `{c}`", c = c).as_str(),
                    "bad char",
                    start,
                    lx.byte,
                ));
            }
        }
        if lx.pos == before && lx.pos < lx.chars.len() {
            let bad = lx.peek().unwrap_or('?');
            lx.bump();
            diags.push(lx.err(
                "E011",
                format!("非法字符 `{bad}`").as_str(),
                format!("bad char `{bad}`").as_str(),
                lx.byte.saturating_sub(bad.len_utf8()),
                lx.byte,
            ));
        }
    }

    LexOut { tokens, diags }
}

fn scan_word(lx: &mut Lexer, tokens: &mut Vec<(Kind, usize)>) {
    let start = lx.byte;
    let mut buf = String::new();
    while let Some(c) = lx.peek() {
        if c.is_alphanumeric() || c == '_' || c == '.' || c == '-' || c == '+' {
            buf.push(c);
            lx.bump();
        } else {
            break;
        }
    }
    let kind = match buf.as_str() {
        "impt" => Kind::Impt,
        "true" => Kind::Bool(true),
        "false" => Kind::Bool(false),
        "null" => Kind::Null,
        _ => Kind::Name(buf),
    };
    tokens.push((kind, start));
}

fn scan_number(lx: &mut Lexer, tokens: &mut Vec<(Kind, usize)>, diags: &mut Vec<Diag>) -> bool {
    let start = lx.byte;
    let start_pos = lx.pos;
    let loc = |lx: &Lexer| loc_of(lx.src, start, lx.byte);
    let mut sign = 1i64;
    if lx.peek() == Some('-') {
        sign = -1;
        lx.bump();
    } else if lx.peek() == Some('+') {
        lx.bump();
    }

    let mut radix = 10u32;
    if lx.peek() == Some('0') {
        if let Some(p) = lx.peek2() {
            let r = match p {
                'x' | 'X' => Some(16),
                'o' | 'O' => Some(8),
                'b' | 'B' => Some(2),
                _ => None,
            };
            if let Some(r) = r {
                radix = r;
                lx.bump();
                lx.bump();
            }
        }
    }

    let mut int_digits = String::new();
    let leading_dot = lx.peek() == Some('.');
    if leading_dot {
        lx.bump();
    }
    while let Some(c) = lx.peek() {
        if c.is_digit(radix) || c == '_' {
            int_digits.push(c);
            lx.bump();
        } else {
            break;
        }
    }
    let bare = int_digits.replace('_', "");

    if !bare.is_empty() && !underscores_ok(&int_digits) {
        diags.push(Diag::error_hint(
            lx.lang,
            "E005",
            "下划线只能夹在数字中间. 写 1_000 别写 1_",
            "bad underscore. write 1_000 not 1_",
            Some("示例: 1_000"),
            Some("e.g. 1_000"),
            loc(lx),
        ));
        return true;
    }

    if bare.is_empty() {
        // not a number. rewind so caller rescans as word.
        lx.byte = start;
        lx.pos = start_pos;
        return false;
    }

    let mut frac = String::new();
    let mut exp = String::new();
    let mut is_float = leading_dot;
    if radix == 10 && lx.peek() == Some('.') {
        let has_digit_after = lx.peek2().map(|c| c.is_ascii_digit()).unwrap_or(false);
        if has_digit_after {
            is_float = true;
            lx.bump();
            while let Some(c) = lx.peek() {
                if c.is_ascii_digit() || c == '_' {
                    frac.push(c);
                    lx.bump();
                } else {
                    break;
                }
            }
            if !frac.is_empty() && !underscores_ok(&frac) {
                diags.push(Diag::error(
                    lx.lang,
                    "E005",
                    "小数里下划线放错了",
                    "bad underscore in fraction",
                    loc(lx),
                ));
                return true;
            }
        } else if is_float_delim(lx.peek2().unwrap_or(' ')) {
            is_float = true;
            lx.bump();
        } else {
            // e.g. version 1.foo. leave whole thing as bare.
            lx.byte = start;
            lx.pos = start_pos;
            return false;
        }
    }
    if radix == 10 && matches!(lx.peek(), Some('e') | Some('E')) {
        is_float = true;
        lx.bump();
        if matches!(lx.peek(), Some('+') | Some('-')) {
            exp.push(lx.peek().unwrap());
            lx.bump();
        }
        let mut has = false;
        while let Some(c) = lx.peek() {
            if c.is_ascii_digit() || c == '_' {
                has = true;
                exp.push(c);
                lx.bump();
            } else {
                break;
            }
        }
        if !has || !underscores_ok(&exp) {
            // bad exp like 1e. rewind, let word scan own it.
            lx.byte = start;
            lx.pos = start_pos;
            return false;
        }
    }
    if radix != 10 && lx.peek() == Some('.') {
        lx.byte = start;
        lx.pos = start_pos;
        return false;
    }

    // trailing garbage like 123abc is a bare word, not a number.
    if let Some(c) = lx.peek() {
        if !is_delim(c) {
            lx.byte = start;
            lx.pos = start_pos;
            return false;
        }
    }

    if !is_float {
        let val = if radix == 10 {
            bare.parse::<i64>().map(|v| v * sign)
        } else {
            i64::from_str_radix(&bare, radix).map(|v| v * sign)
        };
        match val {
            Ok(v) => {
                tokens.push((Kind::Int(v), start));
                true
            }
            Err(_) => {
                diags.push(Diag::error_hint(
                    lx.lang,
                    "E005",
                    "整数坏了或溢出",
                    "bad integer or out of range",
                    Some("整数是 i64"),
                    Some("integers are i64"),
                    loc(lx),
                ));
                true
            }
        }
    } else {
        let frac = frac.trim_matches('_');
        let exp = exp.trim_matches('_');
        let mut mant = String::new();
        if leading_dot {
            mant.push_str("0.");
            mant.push_str(&bare);
        } else {
            mant.push_str(&bare);
            if !frac.is_empty() {
                mant.push('.');
                mant.push_str(frac);
            } else if exp.is_empty() {
                mant.push_str(".0");
            }
        }
        if !exp.is_empty() {
            mant.push('e');
            mant.push_str(exp);
        }
        match mant.parse::<f64>() {
            Ok(v) => {
                tokens.push((Kind::Float(v * sign as f64), start));
                true
            }
            Err(_) => {
                diags.push(Diag::error(
                    lx.lang,
                    "E005",
                    "浮点数坏了",
                    "bad float",
                    loc(lx),
                ));
                true
            }
        }
    }
}

fn underscores_ok(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return true;
    }
    if bytes[0] == b'_' || bytes[bytes.len() - 1] == b'_' {
        return false;
    }
    for i in 0..bytes.len() {
        if bytes[i] == b'_' {
            if i == 0 || i == bytes.len() - 1 {
                return false;
            }
            if bytes[i - 1] == b'_' || bytes[i + 1] == b'_' {
                return false;
            }
        }
    }
    true
}

fn is_delim(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ',' | '[' | ']' | '{' | '}' | ':' | '=' | '"' | '#' | '/' | '\''
        )
}

fn is_float_delim(c: char) -> bool {
    is_delim(c) || matches!(c, 'e' | 'E' | '+' | '-')
}

fn scan_string(lx: &mut Lexer, tokens: &mut Vec<(Kind, usize)>, diags: &mut Vec<Diag>) {
    let start = lx.byte;
    lx.bump();
    let mut buf = String::new();
    loop {
        let c = match lx.peek() {
            Some(c) => c,
            None => {
                diags.push(Diag::error_hint(
                    lx.lang,
                    "E003",
                    "字符串没闭合. 缺结尾引号",
                    "unterminated string. missing quote",
                    Some("结尾加引号. 跨行用 #\"...\"#"),
                    Some("close with quote. multiline uses #\"...\"#"),
                    loc_of(lx.src, start, start + 1),
                ));
                break;
            }
        };
        match c {
            '"' => {
                lx.bump();
                break;
            }
            '\n' => {
                diags.push(Diag::error_hint(
                    lx.lang,
                    "E003",
                    "字符串没闭合就换行了. 单行字符串不能跨行",
                    "string hits newline before closing quote",
                    Some("结尾加引号. 跨行用 #\"...\"#"),
                    Some("close with quote. multiline uses #\"...\"#"),
                    loc_of(lx.src, start, lx.byte),
                ));
                break;
            }
            '\\' => {
                lx.bump();
                let esc_start = lx.byte.saturating_sub(1);
                match lx.peek() {
                    Some('n') => {
                        buf.push('\n');
                        lx.bump();
                    }
                    Some('r') => {
                        buf.push('\r');
                        lx.bump();
                    }
                    Some('t') => {
                        buf.push('\t');
                        lx.bump();
                    }
                    Some('0') => {
                        buf.push('\0');
                        lx.bump();
                    }
                    Some('\\') => {
                        buf.push('\\');
                        lx.bump();
                    }
                    Some('"') => {
                        buf.push('"');
                        lx.bump();
                    }
                    Some('\'') => {
                        buf.push('\'');
                        lx.bump();
                    }
                    Some('x') => {
                        lx.bump();
                        let mut hex = String::new();
                        for _ in 0..2 {
                            if let Some(h) = lx.peek() {
                                if h.is_ascii_hexdigit() {
                                    hex.push(h);
                                    lx.bump();
                                }
                            }
                        }
                        match u8::from_str_radix(&hex, 16) {
                            Ok(b) => buf.push(b as char),
                            Err(_) => diags.push(Diag::error(
                                lx.lang,
                                "E004",
                                "\\x 后面要 2 位十六进制. 如 \\x41",
                                "\\x needs 2 hex digits",
                                loc_of(lx.src, esc_start, lx.byte),
                            )),
                        }
                    }
                    Some('u') => {
                        lx.bump();
                        if lx.peek() != Some('{') {
                            diags.push(Diag::error(
                                lx.lang,
                                "E004",
                                "\\u 写法是 \\u{4E2D}",
                                "\\u looks like \\u{4E2D}",
                                loc_of(lx.src, esc_start, lx.byte),
                            ));
                        } else {
                            lx.bump();
                            let mut hex = String::new();
                            while let Some(h) = lx.peek() {
                                if h == '}' {
                                    break;
                                }
                                if h.is_ascii_hexdigit() {
                                    hex.push(h);
                                    lx.bump();
                                } else {
                                    break;
                                }
                            }
                            if lx.peek() == Some('}') {
                                lx.bump();
                            }
                            match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                                Some(ch) => buf.push(ch),
                                None => diags.push(Diag::error(
                                    lx.lang,
                                    "E004",
                                    "unicode 转义坏了",
                                    "bad unicode escape",
                                    loc_of(lx.src, esc_start, lx.byte),
                                )),
                            }
                        }
                    }
                    other => {
                        let got = other.map(|x| x.to_string()).unwrap_or_default();
                        diags.push(Diag::error(
                            lx.lang,
                            "E004",
                            format!("转义 `\\{got}` 不存在. 可用的是 n r t 0 \\ \" xNN u{{...}}").as_str(),
                            format!("unknown escape `\\{got}`. want n r t 0 \\ \" xNN u{{...}}").as_str(),
                            loc_of(lx.src, esc_start, lx.byte + 1),
                        ));
                        if let Some(_c) = lx.peek() {
                            lx.bump();
                        }
                    }
                }
            }
            '{' => {
                let interp_start = lx.byte;
                lx.bump();
                let name_start = lx.byte;
                while let Some(n) = lx.peek() {
                    if n.is_ascii_alphanumeric() || n == '_' {
                        lx.bump();
                    } else {
                        break;
                    }
                }
                let name: String = lx.src[name_start..lx.byte].to_string();
                if lx.peek() == Some('}') {
                    lx.bump();
                    if name.is_empty() {
                        diags.push(Diag::error_hint(
                            lx.lang,
                            "E006",
                            "插值名是空的. 写 {name} 这种",
                            "empty var. write {name}",
                            Some("格式是 \"hi {name}\""),
                            Some("e.g. \"hi {name}\""),
                            loc_of(lx.src, interp_start, lx.byte),
                        ));
                    } else if lx.keep_interp {
                        // fmt path. leave template alone.
                        buf.push('{');
                        buf.push_str(&name);
                        buf.push('}');
                    } else if let Some(v) = lx.vars.get(&name) {
                        buf.push_str(v);
                    } else {
                        diags.push(Diag::warning_hint(
                            lx.lang,
                            "W001",
                            format!("变量 `{name}` 没定义. 置空了", name = name).as_str(),
                            format!("undefined var `{name}`. left empty", name = name).as_str(),
                            Some("用 --var 名字=值 传进来"),
                            Some("pass --var name=value"),
                            loc_of(lx.src, interp_start, lx.byte),
                        ));
                    }
                } else {
                    diags.push(Diag::error_hint(
                        lx.lang,
                        "E006",
                        "插值没闭合. 缺 `}`",
                        "unclosed var. missing `}`",
                        Some("写法是 \"值 {变量}\""),
                        Some("e.g. \"value {var}\""),
                        loc_of(lx.src, interp_start, lx.byte),
                    ));
                }
            }
            _ => {
                buf.push(c);
                lx.bump();
            }
        }
    }
    tokens.push((Kind::Str(buf), start));
}

fn scan_raw_string(lx: &mut Lexer, tokens: &mut Vec<(Kind, usize)>, diags: &mut Vec<Diag>) {
    let start = lx.byte;
    lx.bump();
    lx.bump();
    let mut buf = String::new();
    loop {
        if lx.peek() == Some('"') && lx.peek2() == Some('#') {
            lx.bump();
            lx.bump();
            break;
        }
        match lx.peek() {
            Some(c) => {
                buf.push(c);
                lx.bump();
            }
            None => {
                diags.push(Diag::error_hint(
                    lx.lang,
                    "E003",
                    "原始字符串没闭合. 缺结尾 \"#",
                    "unclosed raw string. missing \"#",
                    Some("原始字符串是 #\" 开头 \"# 结尾"),
                    Some("raw string is #\" to \"#"),
                    loc_of(lx.src, start, start + 2),
                ));
                break;
            }
        }
    }
    tokens.push((Kind::RawStr(buf), start));
}
