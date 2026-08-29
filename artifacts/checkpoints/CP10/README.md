# CP10 evidence: the CLI teaches its lifecycle

Status: **Passed**

Starting revision: `aa61f681ab5af9ce10d8b6d4020205008831f42b`

Date: 2026-08-28

## Implemented behavior

- Global help draws the durable task state and five agent routes.
- Every command explains when to use it, its state transition, its next safe action, output, and one example.
- `spewer help <command>` and `spewer <command> --help` return the same reference.
- Invalid commands exit with code 2 and direct the caller to `spewer help`.
- Parsing, help text, command execution, and storage behavior remain in separate modules below 500 lines each.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo test --locked` | 0 | 30 tests passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Documentation passed |
| `cargo deny check` | 0 | Dependency policy passed |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |

## Help contract checks

Executable tests assert the global state diagram, the `run` transition, recovery routing, and invalid-command hint.

Unit tests cover both help forms for all nine commands. They also require every command to contain `USAGE`, `WHEN`, `STATE`, `NEXT`, `OUTPUT`, `EXAMPLE`, and a transition arrow.

The desk documentation linter reported no failures across the global help and all nine command pages.

The global help SHA-256 is `73ae9d3b1babe575d2ca5cdb2d487099f541c5f76e64bf2e754bfe6eded48d08`. The ordered command-help SHA-256 is `a68e190dc70b621f403cc9fe0caf9a50b399e05cdf0698b89c53168fe6dde3e3`.

## Lint waivers

- `WAIVE S-01`: State diagrams and aligned command tables contain separate lines that the prose linter joins into one sentence.
- `WAIVE S-04`: Command syntax, JSON field names, and state arrows are fixed protocol terms rather than prose noun clusters.
- `WAIVE T-02`: The command catalog is a reference list rather than a prose paragraph.
- `WAIVE S-05`: The interrupted process is the paragraph topic; no missing actor changes the instruction.

## Known limitations

The current `run` command remains attached until completion or interruption. Asynchronous submission is a later CLI contract change.

Next checkpoint: **JSON/CLI completion gate, including default model selection and detached lifecycle control**
