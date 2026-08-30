# CP26 evidence: first packaged release

Status: **Active**

Target version: **0.2.0**

CP26 packages Spewer for direct download and Homebrew without changing its task protocol.

## Release contract

- Tag `v0.2.0` must match the Cargo package version.
- GitHub Actions must verify formatting, Clippy, and every Cargo test target.
- Native runners must build macOS and Linux archives for ARM64 and x86-64.
- Every archive must contain `spewer`, the `spu` alias, README, and license.
- The GitHub release must include four archives and `SHA256SUMS`.
- `modiqo/homebrew-tap` must install both command names from the release assets.

## Exit gate

The release closes after the GitHub workflow and clean Homebrew installation pass. Both command
names must report version 0.2.0.
