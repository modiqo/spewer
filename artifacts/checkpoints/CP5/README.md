# CP5 evidence: durable completion delivery

Starting revision: `a57dbb538df8076370197b0877c7b7d295268978`.

One SQLite transaction can now commit the terminal event, receipt, and stable outbox message. The transaction is idempotent by source key, task, receipt, and message identity.

The callback test in `tests/recovery_delivery.rs` retries the complete finalization transaction. The second application reuses the first event and callback. Consumer acknowledgements apply once, while an unacknowledged message remains available after restart.

The controlled restart matrix also reads a callback, closes before acknowledgement, reopens, and receives the same message. Its SHA-256 is `3a00235b087798042ce6f1e936b0b80c10332576d1f676d54c1ebe32d07dcd2c`.

A live Luna run completed as task `tsk_78595f1272089d1b049e994d` at event 173. Receipt `rcp_b04d8a2525c341f688316899` and message `msg_358613e23d10ab4eef140739` survived polling. The first acknowledgement applied; the repeated acknowledgement did not.

All 26 tests and every repository gate passed. Stream, wait, and poll requests share the durable outbox; the CLI exposes polling and acknowledgement commands.

Next checkpoint: CP6.
