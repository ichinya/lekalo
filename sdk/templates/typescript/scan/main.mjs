/**
 * The SDK scan example (issue #106): a capability-limited scan-only
 * adapter on `@lekalo/adapter-sdk`. It declares `describe` and `scan`
 * honestly and refuses everything else with a closed in-envelope error;
 * read-only by default, no write scopes, no network, no shell.
 */
import { buildAdapterIdentity, buildCapabilities } from "../../../typescript/src/runtime/build.mjs";
import { sha256 } from "../../../typescript/src/runtime/ids.mjs";
import { serve } from "../../../typescript/src/runtime/serve.mjs";

const IDENTITY = buildAdapterIdentity({
  digest: sha256("lekalo adapter-sdk scan example"),
  id: "sdk-scan-example",
  version: "0.1.0",
});

serve({
  handlers: {
    async describe() {
      return buildCapabilities({
        adapter: IDENTITY,
        capabilities: { "scan.symbols": "full" },
        irVersions: [],
        operations: ["describe", "scan"],
        progress: false,
        protocolVersions: ["0.3.2"],
        readScopes: ["src/**"],
        targets: ["sdk-scan-example"],
        transports: ["stdin", "file"],
      });
    },
    async scan() {
      return {
        payload: {
          entries: [
            { kind: "source", path: "src/main.ts" },
            { kind: "source", path: "src/util.ts" },
          ],
          truncated: false,
        },
      };
    },
  },
  identity: IDENTITY,
});
