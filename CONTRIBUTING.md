# Contributing

Patches welcome. Keep them small and boring.

## Workflow

1. Fork and branch off main.
2. Make your change.
3. Run `cargo test`. It must pass.
4. Run `cargo fmt --check` for Rust code. Fix what it flags.
5. Open a PR against main. Say what broke and how you tested.

## Rules

- One PR does one thing. No drive by refactors.
- Public API, JSON output shape, CLI flags and diagnostic codes like E001 stay stable. If you must break them, say so loudly in the PR.
- New behavior needs a test in `tests/oii.rs`. No test, no merge.
- Keep diagnostics short and actionable. Match existing tone.
- `zh` and `en` messages get updated together. Never leave one stale.
- Code comments are short lowercase English with no trailing period. Linus style. Diagnostic strings keep their punctuation.
- New public API and JSON shape changes are breaking. Note them in the PR and in the README breaking section.

## AI assisted code

AI written code is welcome. Slop is not.

- Read every line you submit. If you did not read it, do not send it.
- AI review does not count as review. A model saying looks good means nothing. A human signs off.
- No need to mark which lines are AI written. Submitted code is yours. Credit and responsibility both land on you.
- Generated tests must fail before the fix and pass after. Show the evidence.
- Keep the diff small. AI loves to reformat the world. Do not let it.

## Filing issues

Use the bug template. Include repro and full logs. Debugging any issue without error logs is like driving blind.
