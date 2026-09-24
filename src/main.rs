use std::collections::HashMap;
use std::io::Read;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

use oii::diag::{Lang, render_diag_with_file};
use oii::{ParseOptions, parse_with};

#[derive(Parser)]
#[command(
    name = "oii",
    version,
    about = "Config language for Xyle. Suffix .oii",
    long_about = "Config language for Xyle, a package manager in development. Suffix .oii\n\nOne rule: braces do nothing, scopes are brackets.\nComments // /// /* */. impt goes first. Nodes are name [ attrs plus child nodes ].\nValues are strings #\"...\"# ints 0xFF floats 1e3 bools null arrays. Double quotes do {var} interpolation."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// parse .oii file, dump ast
    Parse {
        /// file path - means stdin
        file: String,
        /// auto fix append missing ] at end
        #[arg(long)]
        fix: bool,
        /// with --fix, write fixed source back
        #[arg(long)]
        write: bool,
        /// output json on success same as to-json
        #[arg(long)]
        json: bool,
        /// language: zh or en
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
        /// interp var --var name=value repeatable
        #[arg(long = "var", value_parser = parse_kv, value_name = "NAME=VAL")]
        vars: Vec<(String, String)>,
        /// also read env vars as interp vars
        #[arg(long)]
        env: bool,
    },
    /// convert .oii file to json
    ToJson {
        /// file path - means stdin
        file: String,
        #[arg(long)]
        fix: bool,
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
        #[arg(long = "var", value_parser = parse_kv, value_name = "NAME=VAL")]
        vars: Vec<(String, String)>,
        #[arg(long)]
        env: bool,
    },
    /// list funcs with desc
    Funcs {
        /// file path - means stdin
        file: String,
        /// output json
        #[arg(long)]
        json: bool,
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
    },
    /// run a func and print its value
    Run {
        /// file path - means stdin
        file: String,
        /// func name
        func: String,
        /// call arg --arg name=value repeatable
        #[arg(long = "arg", value_parser = parse_kv, value_name = "NAME=VAL")]
        args: Vec<(String, String)>,
        /// output json
        #[arg(long)]
        json: bool,
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
    },
    /// read one value by path e.g server.port
    Get {
        /// file path
        file: String,
        /// dot path
        path: String,
        /// output json
        #[arg(long)]
        json: bool,
    },
    /// write one value by path keeps comments and layout
    Set {
        /// file path
        file: String,
        /// dot path
        path: String,
        /// new value parsed as oii
        value: String,
        /// write back to file
        #[arg(short, long)]
        write: bool,
        /// output json
        #[arg(long)]
        json: bool,
    },
    /// insert a line into a func, node body, or array attr
    Insert {
        /// file path
        file: String,
        /// func name or dot path. e.g. packages or config.packages
        target: String,
        /// raw line or snippet to insert
        line: String,
        /// insert before this existing entry instead of appending
        #[arg(long)]
        before: Option<String>,
        /// write back to file
        #[arg(short, long)]
        write: bool,
    },
    /// remove a func node or attr by path
    Rm {
        /// file path
        file: String,
        /// func name or dot path
        path: String,
        /// write back to file
        #[arg(short, long)]
        write: bool,
    },
    /// replace an array element by index
    SetIndex {
        /// file path
        file: String,
        /// array attr dot path
        path: String,
        /// element index
        index: usize,
        /// new value parsed as oii
        value: String,
        /// write back to file
        #[arg(short, long)]
        write: bool,
    },
    /// insert an array element at index
    InsertIndex {
        /// file path
        file: String,
        /// array attr dot path
        path: String,
        /// element index
        index: usize,
        /// raw line to insert
        line: String,
        /// write back to file
        #[arg(short, long)]
        write: bool,
    },
    /// remove an array element at index
    RmIndex {
        /// file path
        file: String,
        /// array attr dot path
        path: String,
        /// element index
        index: usize,
        /// write back to file
        #[arg(short, long)]
        write: bool,
    },
    /// format normalize to `:` and 2 space indent
    Fmt {
        /// file path empty means stdin
        file: Option<String>,
        /// check only exit 1 on diff
        #[arg(long)]
        check: bool,
        /// write back to file
        #[arg(short, long)]
        write: bool,
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
    },
    /// check file print diags, exit 1 on error
    Check {
        /// file path - means stdin
        file: String,
        /// warnings also fail default only errors fail
        #[arg(long)]
        deny_warnings: bool,
        /// language: zh or en
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
        /// interp var --var name=value repeatable
        #[arg(long = "var", value_parser = parse_kv, value_name = "NAME=VAL")]
        vars: Vec<(String, String)>,
        /// also read env vars as interp vars
        #[arg(long)]
        env: bool,
    },
    /// print shell completion script
    Completion {
        /// shell name
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// run language server on stdio
    Lsp,
    /// re-check a file on change, --fmt also rewrites it
    Watch {
        /// file path
        file: String,
        /// reformat and write on each change
        #[arg(long)]
        fmt: bool,
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum LangArg {
    Zh,
    En,
}

fn parse_kv(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((k, v)) => Ok((k.trim().to_string(), v.to_string())),
        None => Err("bad --var. want --var name=value".to_string()),
    }
}

// parse a value by parsing a one attr doc reuse the real grammar
fn parse_value(s: &str) -> Result<oii::Value, String> {
    let wrapped = format!("x [ v: {s} ]");
    let doc = oii::parse(&wrapped).map_err(|_| format!("bad value `{s}`"))?;
    doc.node("x")
        .and_then(|n| n.get("v"))
        .cloned()
        .ok_or_else(|| format!("bad value `{s}`"))
}

fn resolve_lang(arg: Option<LangArg>) -> Lang {
    match arg {
        Some(LangArg::Zh) => Lang::Zh,
        Some(LangArg::En) => Lang::En,
        None => Lang::from_env(),
    }
}

fn read_input(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("stdin read failed: {e}"))?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("read {path} failed: {e}"))
    }
}

fn var_map(kv: &[(String, String)], use_env: bool) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if use_env {
        for (k, v) in std::env::vars() {
            map.insert(k, v);
        }
    }
    for (k, v) in kv {
        map.insert(k.clone(), v.clone());
    }
    map
}

fn collect_vars(lang: Vec<(String, String)>, env: bool) -> HashMap<String, String> {
    var_map(&lang, env)
}

fn print_diags(src: &str, fixed: Option<&str>, file: &str, lang: Lang, diags: &[oii::diag::Diag]) {
    let shown = fixed.unwrap_or(src);
    // pipes stay clean real files get a prefix
    let name = if file == "-" { None } else { Some(file) };
    for d in diags {
        eprint!("{}", render_diag_with_file(shown, name, d));
    }
    let errs = diags
        .iter()
        .filter(|d| d.level == oii::diag::Level::Error)
        .count();
    let warns = diags
        .iter()
        .filter(|d| d.level == oii::diag::Level::Warning)
        .count();
    if errs + warns > 0 {
        match lang {
            Lang::Zh => eprintln!("共 {errs} 个错误, {warns} 个警告"),
            Lang::En => eprintln!("{errs} error(s), {warns} warning(s)"),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(1u8)
        }
    }
}

fn run(cli: Cli) -> Result<u8, String> {
    match cli.cmd {
        Cmd::Parse {
            file,
            fix,
            write,
            json,
            lang,
            vars,
            env,
        } => {
            let src = read_input(&file)?;
            let opts = ParseOptions {
                vars: collect_vars(vars, env),
                lang: resolve_lang(lang),
                fix,
                keep_interp: false,
            };
            let out = parse_with(&src, &opts);
            print_diags(
                &src,
                out.fixed_source.as_deref(),
                &file,
                opts.lang,
                &out.diagnostics,
            );
            if out.has_errors() {
                return Ok(1);
            }
            if json {
                if let Some(doc) = &out.doc {
                    println!("{}", oii::to_json_string(doc));
                }
            } else if let Some(doc) = &out.doc {
                println!("{:#?}", doc);
            }
            if let Some(fixed) = &out.fixed_source {
                if write {
                    if file == "-" {
                        print!("{fixed}");
                    } else {
                        std::fs::write(&file, fixed)
                            .map_err(|e| format!("write {file} failed: {e}"))?;
                    }
                }
            }
            Ok(if json && out.doc.is_none() { 1 } else { 0 })
        }
        Cmd::ToJson {
            file,
            fix,
            lang,
            vars,
            env,
        } => {
            let src = read_input(&file)?;
            let opts = ParseOptions {
                vars: collect_vars(vars, env),
                lang: resolve_lang(lang),
                fix,
                keep_interp: false,
            };
            let out = parse_with(&src, &opts);
            print_diags(
                &src,
                out.fixed_source.as_deref(),
                &file,
                opts.lang,
                &out.diagnostics,
            );
            match &out.doc {
                Some(doc) => {
                    println!("{}", oii::to_json_string(doc));
                    Ok(0)
                }
                None => Ok(1),
            }
        }
        Cmd::Funcs { file, json, lang } => {
            let src = read_input(&file)?;
            let opts = ParseOptions {
                vars: HashMap::new(),
                lang: resolve_lang(lang),
                fix: false,
                keep_interp: false,
            };
            let out = parse_with(&src, &opts);
            print_diags(&src, None, &file, opts.lang, &out.diagnostics);
            if out.has_errors() {
                return Ok(1);
            }
            let doc = out.doc.expect("no error means doc exists");
            if json {
                let funcs: Vec<serde_json::Value> = doc
                    .funcs
                    .iter()
                    .map(|f| {
                        serde_json::json!({
                            "name": f.name,
                            "desc": f.desc,
                            "params": f.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&funcs).unwrap_or_default()
                );
            } else {
                for f in &doc.funcs {
                    let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
                    match &f.desc {
                        Some(d) => println!("{}({}) - {}", f.name, params.join(", "), d),
                        None => println!("{}({})", f.name, params.join(", ")),
                    }
                }
            }
            Ok(0)
        }
        Cmd::Run {
            file,
            func,
            args,
            json,
            lang,
        } => {
            let src = read_input(&file)?;
            let opts = ParseOptions {
                vars: HashMap::new(),
                lang: resolve_lang(lang),
                fix: false,
                keep_interp: false,
            };
            let out = parse_with(&src, &opts);
            print_diags(&src, None, &file, opts.lang, &out.diagnostics);
            if out.has_errors() {
                return Ok(1);
            }
            let doc = out.doc.expect("no error means doc exists");
            let vals: Vec<oii::Value> = args
                .iter()
                .map(|(_, v)| parse_value(v).unwrap_or_else(|_| oii::Value::Str(v.clone())))
                .collect();
            match oii::eval(&doc, &func, &vals) {
                Ok(eo) => {
                    if json {
                        let j = serde_json::json!({
                            "value": oii::value_json(&eo.value),
                            "output": eo.output,
                            "steps": eo.steps,
                        });
                        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
                    } else {
                        for line in &eo.output {
                            println!("{line}");
                        }
                        println!("{}", oii::fmt::fmt_value(&eo.value));
                    }
                    Ok(0)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    Ok(1)
                }
            }
        }
        Cmd::Get { file, path, json } => {
            let src = read_input(&file)?;
            let df = oii::DocFile::parse(&src)?;
            match df.get(&path) {
                Some(v) => {
                    if json {
                        let j = oii::value_json(v);
                        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
                    } else {
                        println!("{}", oii::fmt::fmt_value(v));
                    }
                    Ok(0)
                }
                None => {
                    eprintln!("no value at `{path}`");
                    Ok(1)
                }
            }
        }
        Cmd::Set {
            file,
            path,
            value,
            write,
            json,
        } => {
            let src = read_input(&file)?;
            let val = parse_value(&value)?;
            let mut df = oii::DocFile::parse(&src)?;
            df.set(&path, &val)?;
            if json {
                let j = serde_json::json!({
                    "path": path,
                    "value": oii::value_json(&val),
                });
                println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
                Ok(0)
            } else if write {
                df.save_as(&file)?;
                Ok(0)
            } else {
                print!("{}", df.text());
                Ok(0)
            }
        }
        Cmd::Insert {
            file,
            target,
            line,
            before,
            write,
        } => {
            let src = read_input(&file)?;
            let mut df = oii::DocFile::parse(&src)?;
            match before {
                Some(a) => df.insert_before(&target, &a, &line)?,
                None => df.insert(&target, &line)?,
            }
            if write {
                df.save_as(&file)?;
                Ok(0)
            } else {
                print!("{}", df.text());
                Ok(0)
            }
        }
        Cmd::Rm { file, path, write } => {
            let src = read_input(&file)?;
            let mut df = oii::DocFile::parse(&src)?;
            df.remove(&path)?;
            if write {
                df.save_as(&file)?;
                Ok(0)
            } else {
                print!("{}", df.text());
                Ok(0)
            }
        }
        Cmd::SetIndex {
            file,
            path,
            index,
            value,
            write,
        } => {
            let src = read_input(&file)?;
            let val = parse_value(&value)?;
            let mut df = oii::DocFile::parse(&src)?;
            df.set_index(&path, index, &val)?;
            if write {
                df.save_as(&file)?;
                Ok(0)
            } else {
                print!("{}", df.text());
                Ok(0)
            }
        }
        Cmd::InsertIndex {
            file,
            path,
            index,
            line,
            write,
        } => {
            let src = read_input(&file)?;
            let mut df = oii::DocFile::parse(&src)?;
            df.insert_index(&path, index, &line)?;
            if write {
                df.save_as(&file)?;
                Ok(0)
            } else {
                print!("{}", df.text());
                Ok(0)
            }
        }
        Cmd::RmIndex {
            file,
            path,
            index,
            write,
        } => {
            let src = read_input(&file)?;
            let mut df = oii::DocFile::parse(&src)?;
            df.remove_index(&path, index)?;
            if write {
                df.save_as(&file)?;
                Ok(0)
            } else {
                print!("{}", df.text());
                Ok(0)
            }
        }
        Cmd::Fmt {
            file,
            check,
            write,
            lang,
        } => {
            let src = match &file {
                Some(p) => read_input(p)?,
                None => read_input("-")?,
            };
            let opts = ParseOptions {
                vars: HashMap::new(),
                lang: resolve_lang(lang),
                fix: false,
                // fmt must not eat {var}
                keep_interp: true,
            };
            let out = parse_with(&src, &opts);
            let name = file.as_deref().unwrap_or("-");
            print_diags(&src, None, name, opts.lang, &out.diagnostics);
            if out.has_errors() {
                return Ok(1);
            }
            let doc = out.doc.as_ref().expect("no error means doc exists");
            let formatted = oii::format_doc(doc);
            if check {
                if formatted != src {
                    eprintln!("needs fmt. run oii fmt");
                    Ok(1)
                } else {
                    Ok(0)
                }
            } else if write {
                if let Some(p) = &file {
                    std::fs::write(p, &formatted).map_err(|e| format!("write {p} failed: {e}"))?;
                    Ok(0)
                } else {
                    print!("{formatted}");
                    Ok(0)
                }
            } else {
                print!("{formatted}");
                Ok(0)
            }
        }
        Cmd::Check {
            file,
            deny_warnings,
            lang,
            vars,
            env,
        } => {
            let src = read_input(&file)?;
            let opts = ParseOptions {
                vars: collect_vars(vars, env),
                lang: resolve_lang(lang),
                fix: false,
                keep_interp: false,
            };
            let out = parse_with(&src, &opts);
            print_diags(&src, None, &file, opts.lang, &out.diagnostics);
            if out.has_errors() {
                return Ok(1);
            }
            if deny_warnings && !out.warnings().is_empty() {
                return Ok(1);
            }
            Ok(0)
        }
        Cmd::Completion { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "oii", &mut std::io::stdout());
            Ok(0)
        }
        Cmd::Lsp => oii::lsp::run()
            .map(|_| 0)
            .map_err(|e| format!("lsp died: {e}")),
        Cmd::Watch { file, fmt, lang } => {
            let lang = resolve_lang(lang);
            eprintln!("watching {file}. ctrl-c to stop");
            let mut last = String::new();
            loop {
                if let Ok(src) = std::fs::read_to_string(&file) {
                    if src != last {
                        last = src.clone();
                        let opts = ParseOptions {
                            vars: HashMap::new(),
                            lang,
                            fix: false,
                            keep_interp: false,
                        };
                        let out = parse_with(&src, &opts);
                        print_diags(&src, None, &file, lang, &out.diagnostics);
                        if fmt && !out.has_errors() {
                            if let Some(doc) = &out.doc {
                                let formatted = oii::format_doc(doc);
                                if formatted != src {
                                    match std::fs::write(&file, &formatted) {
                                        Ok(()) => {
                                            last = formatted;
                                            eprintln!("wrote {file}");
                                        }
                                        Err(e) => eprintln!("write {file} failed: {e}"),
                                    }
                                }
                            }
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
}
