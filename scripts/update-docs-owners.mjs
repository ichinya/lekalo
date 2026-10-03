#!/usr/bin/env node
// Explicit maintenance only. CI calls the verifier, never this writer.
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { ROOT, binary, discoverCommands, commandOwner, contractRecords, protocolRecords, text, sha, validateMetadata } from "./lib/docs-maintenance.mjs";

if (process.argv.slice(2).join(" ") !== "--write") throw new Error("usage: node scripts/update-docs-owners.mjs --write");
const commands = discoverCommands(binary()).map(row=>({...row,owner:commandOwner(row.command),status:"implemented"}));
const globals = ["--help","--version","--json","--no-cache"].map(flag=>({id:`cli:${flag}`,kind:"global",owner:"docs/cli.md",status:"implemented"}));
const records=[...commands,...globals,...contractRecords(),...protocolRecords()];
validateMetadata(records);
writeFileSync(join(ROOT,"docs/documentation-owners.json"),JSON.stringify({formatVersion:1,productVersion:"0.6.3",cliSourceDigest:sha(text("crates/lekalo-cli/src/main.rs").replaceAll("\r\n","\n")),records},null,2)+"\n");
const cli=text("docs/cli.md").split("\n<!-- issue-105-command-index -->")[0];
const index=["", "<!-- issue-105-command-index -->", "## Command index", "", "Status: **Implemented** command handlers for product 0.6.3. Handler existence does not imply every target capability, migration or native execution is available. Each row comes from recursive CLI help and has one owner in [the checked ownership inventory](documentation-owners.json). `init --adopt` is described in [adopt](adopt.md); native execution remains a typed refusal in the production CLI.", "", "| Command | Synopsis from this binary | Reference owner |", "| --- | --- | --- |",...commands.map(x=>`| \`lekalo ${x.command}\` | \`${x.usage.replaceAll("|","\\|")}\` | [reference](${x.owner.startsWith("docs/")?x.owner.slice(5):"../"+x.owner}) |`),""];
writeFileSync(join(ROOT,"docs/cli.md"),cli.trimEnd()+"\n"+index.join("\n"));
console.log(JSON.stringify({ok:true,commands:commands.length,contracts:contractRecords().length,protocols:protocolRecords().length,globals:globals.length}));
