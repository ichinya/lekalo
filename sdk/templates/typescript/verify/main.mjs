/**
 * The SDK verify example (issue #106): a verification adapter on
 * `@lekalo/adapter-sdk`. It declares `describe` and `verify` honestly,
 * reports findings through the typed builder, and never widens its
 * declared read surface. Read-only; no writes; no network; no shell.
 */
import { buildAdapterIdentity, buildCapabilities, buildFinding } from "../../../typescript/src/runtime/build.mjs";
import { sha256 } from "../../../typescript/src/runtime/ids.mjs";
import { serve } from "../../../typescript/src/runtime/serve.mjs";

const IDENTITY = buildAdapterIdentity({
  digest: sha256("lekalo adapter-sdk verify example"),
  id: "sdk-verify-example",
  version: "0.1.0",
});

// The bounded serve loop returns the process exit code: refusal classes
// (malformed input, oversize, output-limit, …) must surface as nonzero
// process status (issue #106 review r2 #7).
process.exitCode = await serve({
  handlers: {
    async describe() {
      return buildCapabilities({
        adapter: IDENTITY,
        capabilities: { "verify.scenarios": "full" },
        irVersions: ["0.2.16"],
        operations: ["describe", "verify"],
        progress: false,
        protocolVersions: ["0.3.2"],
        readScopes: [".lekalo/ir/**"],
        targets: ["sdk-verify-example"],
        transports: ["stdin", "file"],
      });
    },
    async verify({ request }) {
      const missing = (request.ir_path ?? ".lekalo/ir/planner.json") === "" ;
      if (missing) {
        return {
          payload: {
            findings: [buildFinding({ code: "scenario-missing", detail: "no scenarios declared", path: "src/main.ts" })],
            ok: false,
          },
        };
      }
      return { payload: { findings: [], ok: true } };
    },
  },
  identity: IDENTITY,
});

