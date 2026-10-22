#!/usr/bin/env node
// Issue #94 unix-compile gate (finding r3 2): the `#[cfg(unix)]` lock
// code path must compile on the unix target, not only on the host that
// authored it — the E0716 regression survived every Windows gate because
// the unix branch was never compiled there.
//
// On a unix host this is the plain native `cargo check`; on Windows it
// runs the same check inside WSL against the mounted checkout with an
// isolated Linux target directory. A missing WSL toolchain on a local
// Windows host fails the gate — a silent skip would reopen the r3 class.
// Finding r4 2: on a hosted `windows-latest` CI runner there is no WSL
// distro with cargo, and provisioning one is not part of the workflow,
// so the fail-closed probe deterministically broke the build-test matrix
// leg and masked the windows-native format test legs behind it. There
// the gate is explicitly "not applicable" (a documented skip with a
// machine-readable reason): the unix cfg path is natively compiled — and
// this same script therefore fails closed — on the ubuntu and macos
// matrix legs of the same job, which is the coverage the gate exists
// for. A local Windows host without WSL still fails hard.

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const args = ["check", "--workspace", "--all-targets", "--locked"];

const fail = (reason, detail) => {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "unix-compile", reason, detail }, null, 2)}\n`,
  );
  process.exit(1);
};

// Hosted CI detection (finding r4 2): GitHub Actions exports these on
// every hosted runner. Their absence cannot manufacture a skip on a
// developer machine.
const hostedCi = process.env.GITHUB_ACTIONS === "true" && Boolean(process.env.RUNNER_NAME);

if (process.platform === "linux" || process.platform === "darwin") {
  // Native unix host: this IS the unix target.
  const run = spawnSync("cargo", args, { stdio: "inherit" });
  if (run.error) {
    fail("cargo-unreadable", String(run.error));
  }
  if (run.status !== 0) {
    fail("unix-compile-failed", `cargo exited ${run.status}`);
  }
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "unix-compile", host: process.platform }, null, 2)}\n`);
  process.exit(0);
}

if (process.platform === "win32") {
  const probe = spawnSync("wsl.exe", ["-e", "bash", "-lc", "command -v cargo"], {
    encoding: "utf8",
  });
  const toolchainPresent = !probe.error && probe.status === 0;
  if (!toolchainPresent && hostedCi) {
    // Documented not-applicable skip (finding r4 2): a hosted Windows
    // runner has no WSL toolchain by construction, and the unix target
    // is compiled natively on the other matrix legs. The skip reason is
    // machine-readable so a future WSL-provisioned workflow can assert
    // the gate really ran on Windows.
    process.stdout.write(`${JSON.stringify({
      ok: true,
      gate: "unix-compile",
      host: "win32+ci",
      skipped: "unix-toolchain-not-applicable",
      detail: "hosted windows runner has no WSL rust toolchain; the unix cfg path is compiled natively on the ubuntu/macos matrix legs",
    }, null, 2)}\n`);
    process.exit(0);
  }
  if (!toolchainPresent) {
    fail(
      "unix-toolchain-missing",
      "WSL with cargo is required for the unix-target compile gate (install WSL Debian and rustup, then rerun)",
    );
  }
  // Windows host: the unix target compiles inside WSL. The target
  // directory stays on the Linux filesystem, outside the checkout, so
  // ELF products never mix with the Windows build cache.
  const mapped = root.replace(/^([A-Za-z]):/, (_, drive) => `/mnt/${drive.toLowerCase()}`).replaceAll("\\", "/");
  const remote = [
    "set -eu",
    `cd '${mapped}'`,
    'CARGO_TARGET_DIR="$HOME/lekalo-unix-check-target" cargo check --workspace --all-targets --locked',
  ].join(" && ");
  const run = spawnSync("wsl.exe", ["-e", "bash", "-lc", remote], { stdio: "inherit" });
  if (run.error) {
    fail("wsl-unreadable", String(run.error));
  }
  if (run.status !== 0) {
    fail("unix-compile-failed", `WSL cargo exited ${run.status}`);
  }
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "unix-compile", host: "win32+wsl" }, null, 2)}\n`);
  process.exit(0);
}

fail("unsupported-host", `no unix-target compile route for ${process.platform}`);
