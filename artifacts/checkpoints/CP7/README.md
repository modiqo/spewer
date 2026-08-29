# CP7 evidence: Pareto IQ inputs

Starting revision: `a57dbb538df8076370197b0877c7b7d295268978`.

Receipts preserve input, cached input, output, and reasoning tokens separately. They also retain wall time, tool calls, observed model reroutes, derived cost, and the exact price-configuration hash.

`PriceConfig` rejects cost derivation when required usage is missing. `RunExport` retains passed and attempted verification counts. `pareto_points` emits model, cost, both quality counts, and price hash. It rejects mixed task classes unless the caller explicitly overrides that check.

The portability and telemetry test SHA-256 is `812f2635a04a69a1e0964bb3e4bc4a98a056cff1a383f638680e9ac5a32c5be2`. Its two plot-ready fixture points retain their denominators and price provenance. `config/prices.example.json` is test data, not a claim about current provider prices.

A live Luna receipt retained 52,586 input, 44,288 cached input, 292 output, and 29 reasoning tokens. It recorded two tools and 12,196 milliseconds. Cost stayed unknown because no current Luna price file was configured.

All 26 tests and every repository gate passed.

Next checkpoint: CP8.
