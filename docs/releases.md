# Install Lekalo CLI

Download a platform archive and `SHA256SUMS.txt` from the same [GitHub Release](https://github.com/ichinya/lekalo/releases). A ready binary does not require Rust or Node to start; adapter operations and native checks have their own prerequisites.

| Platform | Target | Archive |
| --- | --- | --- |
| Windows x64 | `x86_64-pc-windows-msvc` | ZIP with `lekalo.exe` |
| Linux x64 | `x86_64-unknown-linux-gnu` | tar.gz with `lekalo` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | tar.gz with `lekalo` |
| macOS Intel | `x86_64-apple-darwin` | tar.gz with `lekalo` |
| macOS Apple Silicon | `aarch64-apple-darwin` | tar.gz with `lekalo` |

Names follow `lekalo-v<VERSION>-<TARGET>.<EXTENSION>`. Each archive has one top-level directory containing the executable and README. Linux builds use glibc and are tested on Ubuntu 22.04; they are not musl binaries. [The release workflow](../.github/workflows/release.yml) records the build platforms.

## Verify and install

Download into an empty working directory, then run the matching block there. The examples use `0.8.0`; change the version to the release you downloaded. They select exactly one checksum row and verify it before extraction.

### Windows (PowerShell)

```powershell
$version = '0.8.0'
$archive = "lekalo-v$version-x86_64-pc-windows-msvc.zip"
$entries = @(Get-Content -LiteralPath SHA256SUMS.txt | Where-Object { $_.EndsWith("  $archive") })
if ($entries.Count -ne 1) { throw 'Checksum entry missing or duplicated' }
$expected = ($entries[0] -split '  ', 2)[0]
$actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw 'SHA-256 mismatch' }
Expand-Archive -LiteralPath $archive -DestinationPath .
& ".\lekalo-v$version-x86_64-pc-windows-msvc\lekalo.exe" --version
```

Move `lekalo.exe` to a directory on PATH for invocation from any working directory. Keep another version in a separate directory if you need it for rollback. [PowerShell archive](https://learn.microsoft.com/powershell/module/microsoft.powershell.archive/expand-archive) and [hash](https://learn.microsoft.com/powershell/module/microsoft.powershell.utility/get-filehash) references describe the commands used here.

### Linux (Bash)

```bash
set -euo pipefail
version=0.8.0
target=x86_64-unknown-linux-gnu # aarch64-unknown-linux-gnu для ARM64
archive="lekalo-v${version}-${target}.tar.gz"
awk -v name="$archive" '$2 == name { print; found++ } END { if (found != 1) exit 1 }' SHA256SUMS.txt > selected-checksum.txt
sha256sum --check --strict selected-checksum.txt
tar -xzf "$archive"
mkdir -p "$HOME/.local/bin"
install -m 755 "lekalo-v${version}-${target}/lekalo" "$HOME/.local/bin/lekalo"
"$HOME/.local/bin/lekalo" --version
```

Add `$HOME/.local/bin` to PATH if needed. The install command replaces an existing binary at that destination; choose a version-specific destination to retain both.

### macOS (Bash)

```bash
set -euo pipefail
version=0.8.0
target=aarch64-apple-darwin # x86_64-apple-darwin для Intel
archive="lekalo-v${version}-${target}.tar.gz"
awk -v name="$archive" '$2 == name { print; found++ } END { if (found != 1) exit 1 }' SHA256SUMS.txt > selected-checksum.txt
shasum -a 256 --check selected-checksum.txt
tar -xzf "$archive"
mkdir -p "$HOME/.local/bin"
install -m 755 "lekalo-v${version}-${target}/lekalo" "$HOME/.local/bin/lekalo"
"$HOME/.local/bin/lekalo" --version
```

Add `$HOME/.local/bin` to PATH if needed. Archives are not signed or notarized by this workflow. The install command replaces the destination binary.

Each archive also has a `.build.json` record with source commit, target, Rust version, lockfile/binary/archive SHA-256 and workflow URL. Checksums establish byte integrity; these records are build metadata, not a cryptographic publisher signature.

## Release snapshots

A release archive is built from its tag commit. Read that tag's README, documentation, fixture locks and contracts when using the binary. Documentation on `main` follows later source changes.

The published `v0.8.0` archive uses tag commit `70c11e9325af70820d609c23c5b29c13b5b85da4` from 2026-10-06. The remaining M8 implementation merged later in [PR #161](https://github.com/ichinya/lekalo/pull/161), on 2026-10-23, and is outside that tag. Matching product numbers alone do not make these source revisions identical.

Update manually by downloading and verifying another release. To build the current source instead, follow [contributing](contributing.md#build-from-source). To create release assets, use the separate [maintainer guide](releasing.md).

## Next step

Run `lekalo --version` and `lekalo --help`, then [create a minimal project](project-layout.md#minimal-project) or [choose an adoption path](adoption.md).
