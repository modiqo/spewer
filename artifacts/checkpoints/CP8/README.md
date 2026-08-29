# CP8 evidence: parent integration

Starting revision: `a57dbb538df8076370197b0877c7b7d295268978`.

The parent adapter exposes a bounded handoff, durable event cursor, applied-receipt set, and typed continuation evidence. Reapplying the same receipt does not advance parent state twice.

The Play adapter keeps Play’s continuation identifier inside Play’s owner-private runtime. Spewer receives projected task context only. The integration design SHA-256 is `1f09c4eed148d485d61b06e3e6772d4a9f77f1fd7f7d5f1b506caa081093d00d`.

The portability test covers duplicate parent callbacks and verifies that the Play handoff rejects embedded continuation state. Spewer’s core imports no Play runtime package.

All 25 tests and every repository gate passed.

Next checkpoint: CP9.
