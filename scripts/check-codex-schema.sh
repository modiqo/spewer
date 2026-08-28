#!/bin/sh
set -eu

schema_dir=generated/codex-json
expected_count=295
expected_hash=17d7491e8229234153c74e29a32db4eaed4f01ae0dfb1e90907f3efbe5ed695c

actual_count=$(find "$schema_dir" -type f ! -name manifest.json | wc -l | tr -d ' ')
actual_hash=$(find "$schema_dir" -type f ! -name manifest.json -print0 |
    sort -z | xargs -0 shasum -a 256 | shasum -a 256 | cut -d ' ' -f 1)

if [ "$actual_count" != "$expected_count" ]; then
    printf 'Codex schema count mismatch: expected %s, found %s\n' \
        "$expected_count" "$actual_count" >&2
    exit 1
fi

if [ "$actual_hash" != "$expected_hash" ]; then
    printf 'Codex schema hash mismatch: expected %s, found %s\n' \
        "$expected_hash" "$actual_hash" >&2
    exit 1
fi

printf 'Codex schema manifest: ok\n'
