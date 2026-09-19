use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// editors/ must stay valid. cheap guard against rot.
#[test]
fn vscode_files_parse() {
    let r = root();
    let tm: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(r.join("editors/vscode/syntaxes/oii.tmLanguage.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(tm["scopeName"], "source.oii");
    let txt = tm.to_string();
    for scope in [
        "comment.line.double-slash",
        "keyword.control.import",
        "string.quoted.double",
        "string.unquoted.raw",
        "constant.numeric",
        "variable.interpolation",
        "invalid.illegal.brace",
    ] {
        assert!(txt.contains(scope), "grammar lost {scope}");
    }
    let pkg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(r.join("editors/vscode/package.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(pkg["contributes"]["languages"][0]["id"], "oii");
    assert_eq!(pkg["version"], "0.1.0", "keep in sync with Cargo.toml");
    assert_eq!(pkg["icon"], "icon.svg");
    let lang: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(r.join("editors/vscode/language-configuration.json")).unwrap(),
    )
    .unwrap();
    assert!(lang.get("brackets").is_some());
    let _: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(r.join("editors/vscode/snippets/oii.json")).unwrap(),
    )
    .unwrap();
    for f in ["icon.svg", "README.md", "CHANGELOG.md", ".vscodeignore"] {
        assert!(
            r.join("editors/vscode").join(f).exists(),
            "vscode/{f} gone"
        );
    }
    let svg = std::fs::read_to_string(r.join("editors/vscode/icon.svg")).unwrap();
    assert!(svg.contains("<svg") && svg.contains("</svg>"));
}

#[test]
fn tree_sitter_grammar_sane() {
    let g = std::fs::read_to_string(
        root().join("editors/tree-sitter-oii/grammar.js"),
    )
    .unwrap();
    // doc first. tree-sitter starts at the first rule.
    let doc_at = g.find("    doc:").expect("doc rule gone");
    let impt_at = g.find("    impt:").expect("impt rule gone");
    assert!(doc_at < impt_at, "doc must come before impt");
    // tree-sitter regex has no look-around and no open {n,} repeat.
    assert!(!g.contains("(?!") && !g.contains("(?<"), "look-around banned");
    for q in ["highlights.scm", "folds.scm", "indents.scm"] {
        assert!(
            root()
                .join("editors/tree-sitter-oii/queries")
                .join(q)
                .exists(),
            "query {q} gone"
        );
    }
    assert!(
        !g.contains("(escape)") && !g.contains("(interp)"),
        "opaque string has no inner nodes"
    );
}
