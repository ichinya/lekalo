# Synthetic architecture profile fixtures

Authored issue #84 core-policy fixtures and live-command goldens. The gate copies the synthetic `context-budget/planner` project into an OS temporary directory and runs the built binary there with `--no-cache`. Limits and calibration references are test inputs, not product defaults or empirical evidence. No consumer code, native execution, Mago acceptance or AI task outcomes are represented. Gate authoring mode is explicit; normal acceptance only reads committed fixtures.
