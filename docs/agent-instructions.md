# Install the Lekalo agent instruction

Status: implemented by the explicit `install prompt` command. The English
instruction is embedded in the CLI; installation does not invoke a model or
derive project intent from source code.

The instruction starts with a bounded capsule for the affected semantic IDs.
Full IR, inspect, impact and status are requested when the task needs their
additional evidence, rather than as a mandatory startup checklist. Failed
or incomplete evidence remains visible.

Source already present in the agent context is reused by path, version and
range. When an editor requires its own Read, that should be the first read
for an intended edit. Changed content, missing ranges, a truncated response
or a genuinely replaced context can require a scoped reread with a reason.
Missing contracts or an absent semantic/native join still require relevant
implementation reads; an index alone does not establish behavior.

```sh
lekalo install prompt --project . --dry-run
lekalo install prompt --project .
lekalo install prompt --project . --check
```

The command writes `.agents/lekalo.md`. It adds or updates the Lekalo section
in an existing root `AGENTS.md`, preserving every byte outside its markers.
Use `--create-agents-md` to explicitly create that root file when absent.
The managed section points to the embedded instruction; other agents can
share the same root file.

The prompt records its product version, registry digest and content digest.
An identical installation performs no writes. After upgrading Lekalo,
`--check` reports a missing or stale installation with
`managed.input-invalid` and reason `prompt-stale`. Run the explicit install
command to update it. Neither `status` nor ordinary project commands silently
rewrite instructions.

The exact previous embedded prompt body is recognized by its pinned digest
and can upgrade through the normal explicit install command without
`--force`. This exception does not accept manual edits or arbitrary bodies
with self-declared hashes; those retain the conflict behavior below.

Manual changes to the prompt or its managed root section cause a conflict.
Review the dry-run diff and use `--force` only when replacing those changes is
intended. Duplicate, reversed or incomplete markers remain a refusal even
with `--force`. An instruction whose body is not a recognized embedded
template requires that explicit override; a marker alone does not establish
ownership. Unrelated root content is preserved during a forced update.

Reads and writes refuse symlinks, Windows junctions and hard-linked files.
The installer preflights both destinations, checks their bytes again under
an exclusive installation lock, writes through confined atomic file
replacement, and rolls earlier writes back if a later write fails. Existing
staging files cause a refusal rather than deletion. A crashed installation
can leave `.agents/lekalo-install.lock`; inspect the running processes and
the files before manually removing that lock.

The instruction describes canonical project paths, public read commands,
independent contract versions and the authority boundaries between Lekalo,
OpenSpec, AI Factory and HLV. It grants no additional authorization. The
canonical content is maintained in
[`agent_prompt.md`](../crates/lekalo-cli/src/agent_prompt.md); the installer
does not write separate provider-specific instruction files.

See [CLI](cli.md), [authority](authority.md), [project layout](project-layout.md)
and [diagnostics](diagnostics.md) for the owning reference documents.
