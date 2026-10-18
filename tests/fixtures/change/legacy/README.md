# Genuine r10 no-op upgrade specimen

`r10-no-op-history.json` preserves the real pre-r11 CLI output retained as
`originallyEmittedHistory` in the independent r10 review artifact
`review-73-r10-codex-history-cap-valid-results.json`. It is not a modern
receipt with its operation rows deleted, and none of its fields or public
chain hashes have been repaired. Its 1,748 bytes are the original sorted,
pretty JSON serialization, without a trailing newline, pinned by SHA-256:
`7642c43decb40d5275b4727ab8defb76bafb3e71eda404c5608cbb411d4786a9`.

Provenance is exclusively synthetic: the public `field-vertical` project,
plus a `tasks.act` command with one `title: tasks.text` input. The original
CLI applied an unchanged `operation-input:set` under key `base-noop` and
incorrectly emitted empty record and embedded-receipt operation arrays.
The matching command/request are reproduced in the core integration and
transaction gate tests; no private source, runtime custody, absolute path,
wall clock, or secret is present in this specimen.

R12 deliberately refuses this schema-invalid history with the typed
`change.history-invalid` / `history-schema` diagnostic before custody
initialization or any new publication. Tests preserve all original
source/history/pin bytes, cover absent, legacy-v1 and epoch-v2 custody,
and keep fresh no-op/mixed-batch positive controls schema-valid. A repair
or migration is not inferred from the missing audit operations.
