import { spawnSync } from "node:child_process";

// A disposable replay must never borrow the caller's index or object store.
// Keep filter/config settings: the gate must exercise real clean filtering.
export function fixtureGitEnv(source = process.env) {
  const locations = new Set(["GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"]);
  return Object.fromEntries(Object.entries(source).filter(([key]) => !locations.has(key.toUpperCase())));
}

export function fixtureGit(cwd, argv, stage, env = process.env, timeout = 30000) {
  const result = spawnSync("git", argv, {
    cwd, env: fixtureGitEnv(env), encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
    timeout, killSignal: "SIGKILL", maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) {
    const failure = { gate: "fixture-git", stage, exit: result.status, signal: result.signal, code: result.error?.code ?? null };
    console.error(JSON.stringify(failure));
    throw new Error(`fixture Git failed: ${stage}`);
  }
  return result.stdout;
}
