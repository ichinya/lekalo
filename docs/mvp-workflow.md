# Complete user workflow

Status: **Implemented** for the declared support matrix. This page joins the
existing references into one reproducible synthetic domain/client example.
It uses the ordinary production CLI and untouched manifested adapter.

## Prerequisites

Use the binary and source snapshot from the same release tag. The
[platform archive](releases.md) contains the executable; the matching GitHub
source archive supplies the self-contained adapters/node-typescript package,
the helper scripts and public fixtures. Its committed adapter.mjs needs no
contributor installation. Native execution additionally requires Windows x64,
an explicitly pinned Node 24 runtime and the declared tool store described in
[native gates](native-gates.md#declared-vitest-typescript-and-vite-tools).

The example provisions Vitest 5.0.5, TypeScript 5.9.3 and Vite 8.3.5 from the
committed lock before planning. No dependency installation occurs during
execution. A fresh dedicated output directory retains the resulting plans and
verify reports.

## Reproduce with an extracted release binary

Run from the matching source snapshot. In PowerShell, set LEKALO_BIN to the
actual extracted executable and create dedicated temporary stores:

```powershell
$env:LEKALO_BIN = 'C:\Tools\lekalo-v1.0.0-x86_64-pc-windows-msvc\lekalo.exe'
$runRoot = Join-Path ([IO.Path]::GetTempPath()) ('lekalo-mvp-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $runRoot | Out-Null
npm.cmd install --prefix "$runRoot\ajv" --ignore-scripts --no-audit --no-fund --no-save ajv@8.20.0
$env:LEKALO_AJV_NODE_PATH = "$runRoot\ajv\node_modules"
$env:LEKALO_NODE_TOOLS_HOME = "$runRoot\tools"
$env:LEKALO_NATIVE_REPORTS_DIR = "$runRoot\reports"
node scripts/test-mvp-workflow.mjs --provision
if ($LASTEXITCODE -ne 0) { throw 'MVP workflow failed; inspect the reported status' }
```

The driver creates an owned temporary project, invokes the selected binary for
each operation and removes its working copy. Reports remain in the explicit
output directory. It does not modify an existing application. No Rust build,
contributor fallback or fixture-runner execution switch is used.

## What the workflow proves

1. A discovery-scope document includes the project and excludes generated
   packages/web/out. Adoption preview leaves source/config bytes unchanged.
2. The authored model declares a local score entity, set_value command and
   write effect. Strict validation first refuses missing authorization; a
   policy bound to the actual IR enables validation. No HTTP/database model
   is required.
3. Public scan executes the committed manifested adapter through the CLI.
   Its declared source profile scans domain exports and client preview logic;
   test implementation, generated outputs and the DOM entrypoint are outside
   this scan scope. The workflow confirms four bindings and audits them.
   The native plan separately captures all declared build/test inputs.
4. Strict validate, inspect, impact and context run on that model. Install
   prompt creates the managed agent guide and AGENTS.md reference, then
   checks their version/content pins.
5. Native plan binds source bytes, selected controls, adapter and complete
   preprovisioned tool assets. Verify receives the exact outer digest as a
   separate confirmation and actually runs Vitest, both typechecks and Vite
   build in a disposable confined copy.
6. The retained report joins semantic results before/after execution with
   real command exits, stdout/stderr references and artifacts, original
   verification and cleanup. Every original source/config file survives.

The public commands and control contracts are owned by [adopt](adopt.md),
[public scan](public-typescript-scan.md), [bindings](bindings.md),
[authorization](authorization.md), [orchestration](orchestration.md),
[native gates](native-gates.md) and [agent instructions](agent-instructions.md).

## Support and evidence boundaries

| Surface | Supported result |
| --- | --- |
| Windows x64, pinned Node 24, qualified LPAC/job backend | Actual four-gate execution, measured exits and cleanup |
| Linux/macOS with available read-only adapter confinement | Adoption, scan, bindings and semantic workflow; explicit unsupported native execution |
| Missing approval, changed source/control/tool/adapter | Refusal; no passing native evidence |
| Tool assertion/typecheck/build error | Failed report with the actual tool code, independently of aggregate CLI exit |
| Timeout, cancellation, output/resource or cleanup defect | Explicit failure class; a zero tool exit cannot mask it |

The companion node-native-tools gate exercises real Vitest/typecheck/Vite
failures and forged-receipt/approval/source refusals. The production-native-
execution gate independently checks the sandbox and its resource/custody
boundaries. CI retains versioned JSON evidence for its exact candidate.
Each acceptance case has separate input, invocation and verify files. The native
tools summary lists SHA-256 digests over the raw input bytes and canonical JSON
request/report for every case, so the saved positive, failure and refusal records
can be checked independently.

This synthetic workflow does not rerun a private consumer or establish
cross-language behavioral equivalence or causal agent productivity gains.
Historical pilot/evaluation acceptance remains linked from the owning issues;
simulation, measured local results and external campaigns retain distinct
evidence states.
