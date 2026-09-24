/**
 * The PostgreSQL storage extension (issue #69, plan S6 spike).
 *
 * One standalone launch-seam extension descriptor that declares the
 * `generate.storage-ddl` capability on the `generate` operation: the
 * adapter can apply a core-proposed, digest-addressed storage DDL or
 * migration plan document. The descriptor carries no rendering logic —
 * core renders the deterministic plan, the adapter applies it under
 * its declared capability, and the plan's `planId` stays the apply
 * authority (the native-gate custody pattern). This spike is
 * standalone by design: nothing in the kernel imports or registers it
 * yet, and wiring the registration (plus the apply pipeline) is the
 * coordinator-owned integration step. The descriptor validates
 * against the kernel's `validateExtensionDescriptor` vocabulary today
 * (pinned by the kernel suite).
 */

/** The one capability this extension declares. */
export const STORAGE_DDL_CAPABILITY = "generate.storage-ddl";

/** The one operation this extension serves. */
export const STORAGE_DDL_OPERATION = "generate";

/** The contract versions of the documents this extension accepts. */
export const ACCEPTED_STORAGE_VERSIONS = Object.freeze([
  "0.4.0",
]);

/**
 * Build the validated extension descriptor. The apply callback is
 * injected by the embedder; the descriptor itself is pure
 * declaration. Throws `RequestRefusal("extension-invalid", …)` through
 * the kernel when the capability vocabulary does not know the id.
 *
 * @param {() => Promise<{applied: number, planId: string}>} apply
 *        the embedder-owned apply entry point.
 */
export function createPostgresStorageExtension(apply) {
  if (typeof apply !== "function") {
    throw new TypeError("the apply entry point must be a function");
  }
  return {
    id: "postgres-storage",
    version: "0.4.0",
    operations: [STORAGE_DDL_OPERATION],
    namedCapabilities: {
      [STORAGE_DDL_CAPABILITY]: "full",
    },
    acceptedIrVersions: [...ACCEPTED_STORAGE_VERSIONS],
    invoke: async (request) => {
      const planId = request?.native_request?.plan_id
        ?? request?.data?.planId;
      if (typeof planId !== "string" || !planId.startsWith("sha256:")) {
        return {
          state: "unsupported",
          diagnostics: [
            {
              id: "storage-engine.migration-gated",
              code: "LEK-SEN-009",
              severity: "error",
              category: "semantic",
              message_id: "storage-engine.migration-gated",
              message:
                "A destructive migration step is blocked until the exact plan id is named.",
              data: { detail: "plan-id-missing" },
              related_locations: [],
              causes: [],
              fixes: [],
              metadata: {},
            },
          ],
        };
      }
      const outcome = await apply();
      return {
        state: "complete",
        data: { ...outcome, planId },
      };
    },
  };
}
