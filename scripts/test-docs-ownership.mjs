#!/usr/bin/env node
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { ROOT, P0, text, sha, safePath, binary, discoverCommands, commandOwner, contractRecords, protocolRecords, validateMetadata, validatePageOwners, assertPublicText, filesUnder } from "./lib/docs-maintenance.mjs";
import { join, relative, resolve, dirname } from "node:path";

const args=process.argv.slice(2);
assert.ok(args.length===0 || args.join(" ")==="--static", "unknown ownership arguments");
const metadata=JSON.parse(text("docs/documentation-owners.json"));
assert.deepEqual(Object.keys(metadata).sort(),["cliSourceDigest","formatVersion","p0Pages","productVersion","records"]);
assert.equal(metadata.formatVersion,1);
assert.equal(metadata.cliSourceDigest,sha(text("crates/lekalo-cli/src/main.rs").replaceAll("\r\n","\n")),"CLI changed without owner refresh");
assert.equal(metadata.productVersion,text("Cargo.toml").match(/\[workspace.package\][\s\S]*?version = "([^"]+)"/)?.[1]);
validatePageOwners(metadata.p0Pages);
validateMetadata(metadata.records);
assert.deepEqual(metadata.records.filter(x=>x.kind==="global"),["--help","--version","--json","--no-cache"].map(flag=>({id:`cli:${flag}`,kind:"global",owner:"docs/cli.md",status:"implemented"})),"global ownership drift");
assert.deepEqual(metadata.records.filter(x=>x.kind==="contract"),contractRecords(),"contract ownership inventory drift");
assert.deepEqual(metadata.records.filter(x=>x.kind==="protocol"),protocolRecords(),"protocol ownership inventory drift");
if (!args.length) assert.deepEqual(metadata.records.filter(x=>x.kind==="command"),discoverCommands(binary()).map(x=>({...x,owner:commandOwner(x.command),status:"implemented"})),"real CLI help ownership drift");
for(const path of P0) {
  const doc=text(path);
  assert.match(doc,/Status:|\*\*Implemented\*\*/,`missing status: ${path}`);
  for(const [,target] of doc.matchAll(/\]\(([^)]+)\)/g)) {
    if(target.startsWith("https://")) continue;
    const [local,anchor]=target.split("#");
    const resolved=resolve(dirname(safePath(path)),decodeURIComponent(local||path.split("/").at(-1)));
    assert.ok(existsSync(decodeURIComponent(resolved)),`broken local link ${path}: ${target}`);
    if(anchor && resolved.endsWith(".md")) {
      const targetText=text(relative(ROOT,resolved).replaceAll("\\","/"));
      const anchors=[...targetText.matchAll(/^#{1,6} (.+)$/gm)].map(x=>x[1].toLowerCase().replace(/[^\p{L}\p{N}\s-]/gu,"").trim().replace(/\s+/g,"-"));
      assert.ok(anchors.includes(anchor),`broken anchor ${path}: ${target}`);
    }
  }
}
assert.match(text("docs/model.md"),/^## Glossary$/m);
// All tracked public prose is checked, including milestone records; there is no
// blanket historical-doc exclusion. This cannot detect unknown private names.
for(const path of [join(ROOT,"README.md"),...filesUnder(join(ROOT,"docs")).filter(x=>x.endsWith(".md"))]) assertPublicText(text(relative(ROOT,path).replaceAll("\\","/")));
for(const canary of ["https://private.example/consumer","consumer-private-canary","credential-canary-105","tenant-private-canary","mysql://user:password@private.example/db"]) assert.throws(()=>assertPublicText(canary));
assert.throws(()=>safePath("../escape"));
assert.throws(()=>validateMetadata([metadata.records[0],metadata.records[0]]));
let pageOwnerControls=0;
const refusePages=(pages,reason)=>{assert.throws(()=>validatePageOwners(pages),reason); pageOwnerControls++;};
for(const page of P0) {
  refusePages(metadata.p0Pages.filter(row=>row.page!==page),/missing P0 page owner/);
  refusePages([...metadata.p0Pages,metadata.p0Pages.find(row=>row.page===page)],/duplicate P0 page owner/);
}
refusePages(undefined,/P0 page owners must be an array/);
refusePages([],/missing P0 page owner/);
for(const [patch,reason] of [
  [{page:"docs/unknown.md"},/unknown P0 page/],
  [{owner:""},/invalid P0 subsystem owner/],
  [{owner:["one","two"]},/invalid P0 subsystem owner/],
  [{extra:true},/unknown\/missing P0 page owner field/],
  [{owner:"unassigned maintainers"},/P0 prose owner drift/],
]) {
  const pages=structuredClone(metadata.p0Pages); Object.assign(pages[0],patch); refusePages(pages,reason);
}
console.log(JSON.stringify({ok:true,gate:"docs-ownership",mode:args.length?"static":"live-help",surfaces:metadata.records.length,requiredDocs:P0.length,p0Owners:metadata.p0Pages.length,pageOwnerControls}));
