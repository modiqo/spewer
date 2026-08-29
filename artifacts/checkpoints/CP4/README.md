# CP4 evidence: interrupted-run recovery

Starting revision: `a57dbb538df8076370197b0877c7b7d295268978`.

Spewer now creates resumable checkpoints that bind its event cursor to the Codex thread and workspace diff. `spewer recover` lists nonterminal tasks. `spewer resume <task-id>` validates the workspace, reads and restores the stored thread, starts a bounded continuation turn, and produces the normal durable receipt.

The recovery test SHA-256 is `c41199ddd609d9819f77253052bbf78d6958095ff896d32561ce24b53a7b127b`. It proves that an unchanged checkpoint validates and that any later workspace diff blocks recovery. The controlled restart matrix SHA-256 is `3a00235b087798042ce6f1e936b0b80c10332576d1f676d54c1ebe32d07dcd2c`.

All 26 tests passed. Formatting, strict Clippy, Rustdoc, dependency policy, source-size, panic-safety, documentation, and Codex schema gates passed. No live Codex recovery test ran at CP4; CP2 already proved the installed App Server contract.

Next checkpoint: CP5.
