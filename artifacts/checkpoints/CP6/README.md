# CP6 evidence: budgets and authority

Starting revision: `a57dbb538df8076370197b0877c7b7d295268978`.

The budget evaluator checks wall time, reported tokens, tools, retries, and derived cost in a fixed order. Runtime token, tool, and wall breaches interrupt Codex and produce `budget.exceeded`. A final cost breach escalates the receipt after provider-usage reconciliation.

The budget implementation SHA-256 is `530c2879b9ddd2e1d776e6ddfadf45e87f48883fe6b1d4cfc31200fd1a44b655`. Missing usage remains unknown.

The security implementation SHA-256 is `d150c32633ce79acceb44f0208ece19eaded5ea554c05fa7c6f23db81983755f`. Tests cover nested secret-key redaction, changed-action approval replay, terminal effect replay, relative-path escape rejection, workspace boundaries, and explicit environment inheritance. The test suite opens no network connection.

A live Luna run with a 20,000-token limit observed 35,981 counted tokens and returned `escalated`. The workspace evidence remained intact.

All 26 tests and every repository gate passed.

Next checkpoint: CP7.
