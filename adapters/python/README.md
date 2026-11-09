# Python observed adapter

Read-only, static `.py`/`.pyi` declaration evidence for the existing
`lekalo scan` and binding registry. This is a separate Rust workspace;
Python/application execution and source generation are outside the scanner.
The separately enabled [FastAPI provider](../../docs/public-fastapi-scan.md)
adds static HTTP registration evidence under an explicit framework policy.

Use the [public Python scan guide](../../docs/public-python-scan.md) for exact
toolchain requirements, locked offline builds, a disposable example and the
`scan → propose → confirm → inspect/impact/context` acceptance recipe.
