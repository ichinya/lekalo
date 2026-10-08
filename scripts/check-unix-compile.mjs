#!/usr/bin/env node
// Issue #94 unix-compile gate (finding r3 2): the `#[cfg(unix)]` lock
// code path must compile on the unix target, not only on the host that
// authored it — the E0716 regression survived every Windows gate because
// the unix branch was never compiled there. The check is mandatory and
// fail-closed: on a unix host it is the plain native `cargo check`; on
// Windows it runs the same check inside WSL against the mounted
// checkout with an isolated Linux target directory. A missing toolchain
// fails the gate — there is no skip-success path.

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
  // Windows host: the unix target compiles inside WSL. The target
  // directory stays on the Linux filesystem, outside the checkout, so
  // ELF products never mix with the Windows build cache.
  const probe = spawnSync("wsl.exe", ["-e", "bash", "-lc", "command -v cargo"], {
    encoding: "utf8",
  });
  if (probe.error || probe.status !== 0) {
    fail(
      "unix-toolchain-missing",
      "WSL with cargo is required for the unix-target compile gate (install WSL Debian and rustup, then rerun)",
    );
  }
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
