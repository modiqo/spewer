# CP26 evidence: first packaged release

Status: **Complete**

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

## Recorded evidence

- Release source: commit `526fd9857b5d850dba3f19fa8f43452c92b82ad5`, tagged `v0.2.0`.
- Four-platform dry run: [GitHub Actions run 33294767532](https://github.com/modiqo/spewer/actions/runs/33294767532).
- Published release run: [GitHub Actions run 33294962767](https://github.com/modiqo/spewer/actions/runs/33294962767).
- Release: [Spewer 0.2.0](https://github.com/modiqo/spewer/releases/tag/v0.2.0), with four native archives and `SHA256SUMS`.
- Homebrew formula: [tap commit e2fdd2f](https://github.com/modiqo/homebrew-tap/commit/e2fdd2f390b09e6bb9bf26523415ed485614205e).
- Homebrew verification: style and new-formula audit passed; a fresh
  `brew install modiqo/tap/spewer` and `brew test modiqo/tap/spewer` passed.
- Installed `/opt/homebrew/bin/spewer --version` and `/opt/homebrew/bin/spu --version` both
  reported `spewer 0.2.0`.

Release SHA-256 values:

```text
845b820dd19642ba50cb62874c090555b3029493fc8096df214fd42311236a36  spewer-v0.2.0-aarch64-apple-darwin.tar.gz
c28e4138e01628a465acbaf096135338a23f018f88091d2d3c4163edb01ffb62  spewer-v0.2.0-aarch64-unknown-linux-gnu.tar.gz
ec5cfcba567f1d91f01a57259a74168593236613364cc4fee279965d4cd89c95  spewer-v0.2.0-x86_64-apple-darwin.tar.gz
4d6f4530bb4f20209c82dc20c8ed4303ce0f96978fa9dc07c07c07a5ac1193bc  spewer-v0.2.0-x86_64-unknown-linux-gnu.tar.gz
```
