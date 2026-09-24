use crate::diag::{Diag, Lang, loc_of};

// count brackets outside strings and comments so we can append missing ]
pub fn apply_auto_fix(src: &str, lang: Lang) -> (String, Vec<Diag>) {
    let cs: Vec<char> = src.chars().collect();
    let mut open = 0usize;
    let mut close = 0usize;
    let mut i = 0usize;
    while i < cs.len() {
        match cs[i] {
            '"' => {
                if cs.get(i + 1) == Some(&'"') && cs.get(i + 2) == Some(&'"') {
                    // multiline cooked
                    i += 3;
                    while i + 2 < cs.len()
                        && !(cs[i] == '"' && cs[i + 1] == '"' && cs[i + 2] == '"')
                    {
                        i += 1;
                    }
                    i = (i + 3).min(cs.len());
                } else {
                    i += 1;
                    while i < cs.len() {
                        if cs[i] == '\\' {
                            i += 2;
                            continue;
                        }
                        if cs[i] == '"' || cs[i] == '\n' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                }
            }
            '#' => {
                let mut h = 0usize;
                let mut j = i;
                while j < cs.len() && cs[j] == '#' {
                    h += 1;
                    j += 1;
                }
                if cs.get(j) == Some(&'"') {
                    if cs.get(j + 1) == Some(&'"') && cs.get(j + 2) == Some(&'"') {
                        // raw multiline. close is """ then h hashes
                        j += 3;
                        while j < cs.len() {
                            if cs[j] == '"'
                                && cs.get(j + 1) == Some(&'"')
                                && cs.get(j + 2) == Some(&'"')
                            {
                                let mut k = j + 3;
                                let mut n = 0;
                                while k < cs.len() && cs[k] == '#' && n < h {
                                    k += 1;
                                    n += 1;
                                }
                                if n == h {
                                    j = k;
                                    break;
                                }
                            }
                            j += 1;
                        }
                        i = j;
                    } else {
                        // raw single. close is " then h hashes
                        j += 1;
                        while j < cs.len() {
                            if cs[j] == '"' {
                                let mut k = j + 1;
                                let mut n = 0;
                                while k < cs.len() && cs[k] == '#' && n < h {
                                    k += 1;
                                    n += 1;
                                }
                                if n == h {
                                    j = k;
                                    break;
                                }
                            }
                            j += 1;
                        }
                        i = j;
                    }
                } else {
                    // #inf #-inf #nan or a bad hash. skip the word
                    i = j;
                    while i < cs.len() && (cs[i].is_ascii_alphabetic() || cs[i] == '-') {
                        i += 1;
                    }
                }
            }
            '/' if cs.get(i + 1) == Some(&'/') => {
                while i < cs.len() && cs[i] != '\n' {
                    i += 1;
                }
            }
            '/' if cs.get(i + 1) == Some(&'*') => {
                i += 2;
                let mut prev = '\0';
                while i < cs.len() {
                    let d = cs[i];
                    i += 1;
                    if d == '/' && prev == '*' {
                        break;
                    }
                    prev = d;
                }
            }
            '[' => {
                open += 1;
                i += 1;
            }
            ']' => {
                close += 1;
                i += 1;
            }
            _ => i += 1,
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
