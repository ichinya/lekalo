# @lekalo/adapter-sdk

Adapter SDK for the `lekalo.target/v1` process protocol (issue #106):
strict wire decoding, semantic validation, honest response builders with
a validate-before-write boundary, deterministic ids and canonical bytes,
path/scope helpers, bounded stderr logging, and the per-exchange serve
loop.

- Supported protocol versions: `0.3.2` and `0.6.4` (exact set — see
  `compatibility.json`).
- No telemetry, no network, no shell. Adapters stay read-only by
  default, declare their scopes, and live inside the host's confinement.
- Zero runtime dependencies; Node >= 24.

## Serve one exchange

```js
import { buildAdapterIdentity, buildCapabilities, serve, sha256 } from "@lekalo/adapter-sdk";

const identity = buildAdapterIdentity({
  digest: sha256("my adapter"),
  id: "my-adapter",
  version: "0.1.0",
});

// The bounded serve loop returns the process exit code; propagation is
// required so refusal classes surface as nonzero process status.
process.exitCode = await serve({
  identity,
  handlers: {
    describe: () =>
      buildCapabilities({
        adapter: identity,
        operations: ["describe", "scan"],
        protocolVersions: ["0.3.2"],
        transports: ["stdin", "file"],
        progress: false,
      }),
    scan: () => ({
      payload: { entries: [], truncated: false },
    }),
  },
});
```

The host owns every decision: it negotiates the exact protocol version,
polices declared scopes, confines the process, and consumes write-plan
authority. The SDK can never widen host policy — it only keeps honest
adapters honest. See `docs/adapter-sdk.md` in the lekalo repository for
the full author guide, `spec.md` for the language-neutral contract, and
`generation.json` for the reproducibility record of this build.
