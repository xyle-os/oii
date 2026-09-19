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
    about = "OII config format. Suffix .oii",
    long_about = "OII config format. Suffix .oii\n\nOne rule: braces do nothing, scopes are brackets.\nComments // /// /* */. impt goes first. Nodes are name [ attrs plus child nodes ].\nValues are strings #\"...\"# ints 0xFF floats 1e3 bools null arrays. Double quotes do {var} interpolation."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse .oii file, dump AST
    Parse {
        /// File path. - means stdin
        file: String,
        /// Auto fix. Append missing ] at end
        #[arg(long)]
        fix: bool,
        /// With --fix, write fixed source back
        #[arg(long)]
        write: bool,
        /// Output JSON on success. Same as to-json
        #[arg(long)]
        json: bool,
        /// Language: zh or en
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
        /// Interp var. --var name=value. Repeatable
        #[arg(long = "var", value_parser = parse_kv, value_name = "NAME=VAL")]
        vars: Vec<(String, String)>,
        /// Also read env vars as interp vars
        #[arg(long)]
        env: bool,
    },
    /// Convert .oii file to JSON
    ToJson {
        /// File path. - means stdin
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
    /// Format. Normalize to `:` and 2 space indent
    Fmt {
        /// File path. Empty means stdin
        file: Option<String>,
        /// Check only. Exit 1 on diff
        #[arg(long)]
        check: bool,
        /// Write back to file
        #[arg(short, long)]
        write: bool,
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
    },
    /// Check file. Print diags, exit 1 on error
    Check {
        /// File path. - means stdin
        file: String,
        /// Warnings also fail. Default only errors fail
        #[arg(long)]
        deny_warnings: bool,
        /// Language: zh or en
        #[arg(long, value_enum, value_name = "LANG")]
        lang: Option<LangArg>,
        /// Interp var. --var name=value. Repeatable
        #[arg(long = "var", value_parser = parse_kv, value_name = "NAME=VAL")]
        vars: Vec<(String, String)>,
        /// Also read env vars as interp vars
        #[arg(long)]
        env: bool,
    },
    /// Print shell completion script
    Completion {
        /// Shell name
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Run language server on stdio
    Lsp,
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
    // pipes stay clean. real files get a prefix.
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
        Cmd::Lsp => oii::lsp::run().map(|_| 0).map_err(|e| format!("lsp died: {e}")),
    }
}
