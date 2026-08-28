#!/bin/sh
set -eu

pattern='\.(unwrap|expect)[[:alnum:]_]*[[:space:]]*\(|\b(panic|todo|unimplemented|unreachable)![[:space:]]*\(|process::abort[[:space:]]*\('

if matches=$(rg -n "$pattern" src tests --glob '*.rs'); then
    printf '%s\n' 'forbidden panic primitive found in Rust source:' >&2
    printf '%s\n' "$matches" >&2
    exit 1
fi

printf '%s\n' 'Rust panic primitive audit: ok'

