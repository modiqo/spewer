//! First-run and capsule-management help.

pub(super) const INSTALL: &str = r"spewer install - prepare one ready detached Luna worker

USAGE
  spewer install [--workspace <path>] [--max-workers <count>]
  spewer install --skip-codex-install

WHEN
  Use for first setup or to verify and reuse an existing installation.

STATE
  host -> Codex CLI -> private defaults -> generic default capsule -> ready service
  Existing configuration and a running service are preserved.

NEXT
  Use 'spewer capsule list' to inspect workers or 'spewer ask <question> --detach'.
  If sign-in is required, run 'codex', then repeat 'spewer install'.

OUTPUT
  One JSON object with readiness, Codex version, capsule catalog, service socket, and next commands.

EXAMPLE
  spewer install --workspace /absolute/path/to/repository
";

pub(super) const CAPSULE: &str = r"spewer capsule - add, discover, or specialize a worker

USAGE
  spewer capsule add <capsule-id> --engine ollama --model <name>
  spewer capsule list
  spewer capsule bind <capsule-id> <skill-or-directory>
  spewer capsule unbind <capsule-id>

WHEN
  Add an installed Ollama model once. List before routing. Bind a valid SKILL.md to specialize a capsule.

STATE
  add: installed Ollama model -> generic capsule
  bind: generic capsule -> specialized capsule
  unbind: specialized capsule -> generic capsule
  list: no state change

NEXT
  Use 'spewer capabilities' from a running adapter to observe the same live catalog.

OUTPUT
  One JSON catalog or one changed manifest. Local skill source paths only appear in administration output.

EXAMPLE
  spewer capsule add qwen3-local --engine ollama --model qwen3:30b-a3b
  spewer capsule bind default ./skills/code-review
  spewer capsule list
";

pub(super) const INIT: &str = r#"spewer init - create private defaults for one-off questions

USAGE
  spewer init [--workspace <path>] [--overwrite]

WHEN
  Use for manual configuration. New users can run 'spewer install' instead.

STATE
  no local configuration -> owner-private ~/.spewer/config.json
  existing configuration -> confirmed replacement | unchanged cancellation

NEXT
  Review the configuration, then use 'spewer ask "<question>"'.
  Use '--overwrite' only when the existing defaults should be replaced.

OUTPUT
  One JSON object with the configuration path and next command.

EXAMPLE
  spewer init --workspace /absolute/path/to/repository
  spewer init --overwrite
"#;
