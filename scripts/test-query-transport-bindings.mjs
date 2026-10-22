#!/usr/bin/env node
// Exercise the real CLI over exact successor Model/query/transport pins.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { basename, dirname, join, resolve } from "node:path";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const require=createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version,"8.20.0");
const Ajv=require("ajv/dist/2020").default;
const root=resolve(dirname(fileURLToPath(import.meta.url)),"..");
const bin=process.env.LEKALO_BIN ?? join(root,"target/debug",process.platform==="win32"?"lekalo.exe":"lekalo");
assert(existsSync(bin),"build the checkout CLI first");
const ajv=new Ajv({strict:true,allErrors:true});
const schema=ajv.compile(JSON.parse(readFileSync(join(root,"contracts/transport-http.schema.v0.8.0.json"),"utf8")));
const stage=realpathSync.native(mkdtempSync(join(tmpdir(),"lekalo-query-transport-")));
const sha=bytes=>"sha256:"+createHash("sha256").update(bytes).digest("hex");
let calls=0;
function run(args,expected=0,detail) {
  calls++;
  const marker=args.indexOf("--");
  const argumentsWithGlobals=marker<0?[...args,"--project",".","--no-cache","--json"]:[...args.slice(0,marker),"--project",".","--no-cache","--json",...args.slice(marker)];
  const result=spawnSync(bin,argumentsWithGlobals,{cwd:stage,encoding:"utf8",timeout:30000,env:{...process.env,LEKALO_PROJECT:""}});
  assert.equal(result.error,undefined);
  assert.equal(result.status,expected,result.stderr||result.stdout);
  const wire=(result.stdout||result.stderr).trimEnd();
  if(detail) assert(wire.includes(detail),wire);
  return {wire,doc:JSON.parse(wire)};
}
const write=(path,value)=>writeFileSync(join(stage,path),JSON.stringify(value));
function watch(expected) {
  calls++;
  const result=spawnSync(bin,["watch","--backend","poll","--json-lines","--stop-after-runs","1","--validate","--project",".","--no-cache"],{cwd:stage,encoding:"utf8",timeout:30000,env:{...process.env,LEKALO_PROJECT:""}});
  assert.equal(result.error,undefined);
  const events=result.stdout.trim().split(/\r?\n/).map(line=>JSON.parse(line));
  const terminal=events.find(row=>row.event==="run-completed");
  assert(terminal,result.stderr||result.stdout);
  assert.equal(terminal.result.status,expected.status);
  assert.deepEqual(terminal.result.diagnostics,expected.diagnostics??[]);
}
try {
  cpSync(join(root,"tests/fixtures/change/field-vertical"),stage,{recursive:true,filter:path=>basename(path)!==".lekalo"});
  const files=["lekalo/project.yaml","lekalo/modules/tasks/module.yaml","lekalo/modules/tasks/entities.yaml"];
  for(const file of files) writeFileSync(join(stage,file),readFileSync(join(stage,file),"utf8").replaceAll("0.2.16","0.8.0"));
  const queries="lekalo/modules/tasks/queries.yaml", endpoints="lekalo/modules/tasks/bindings.yaml";
  writeFileSync(join(stage,queries),"schema_version: '0.8.0'\ndefinitions:\n  - id: tasks.find\n    kind: query\n    version: 1\n    input:\n      - name: title\n        type: tasks.text\n        required: true\n        default: sample\n      - name: tags\n        type: 'list<tasks.text>?'\n        default: null\n    reads:\n      - tasks.task\n    returns: tasks.task\n");
  writeFileSync(join(stage,endpoints),"schema_version: '0.8.0'\ndefinitions:\n  - id: tasks.find_http\n    kind: endpoint\n    version: 1\n    method: GET\n    path: /tasks\n    invokes: tasks.find\n");
  files.push(queries,endpoints);
  const before=files.map(file=>sha(readFileSync(join(stage,file))));
  const model=run(["load"]), ir=run(["load","--ir"]);
  assert.equal(model.wire,JSON.stringify(model.doc));
  assert.equal(ir.wire,JSON.stringify(ir.doc));
  const modelRef={modelVersion:"0.8.0",digest:sha(model.wire)};
  const irRef={identity:"dev.lekalo.ir@0.8.0",digest:sha(JSON.stringify(ir.doc.ir))};
  const query={schemaVersion:"lekalo/query-model/v0.8.0",identity:"dev.lekalo.query-model@0.8.0",attachmentRevision:"1.0.0",projectId:"tasks",modelRef,irRef,tenancy:[],queries:[{query:"tasks.find",source:"tasks.task",cardinality:"one",consistency:"strong",parameters:[{name:"title",type:"tasks.text",required:true,default:"sample"},{name:"tags",type:"list<tasks.text>?",required:false,default:null}]}]};
  const transport={schemaVersion:"lekalo/transport-http/v0.8.0",identity:"dev.lekalo.transport-http@0.8.0",attachmentRevision:"1.0.0",projectId:"tasks",modelRef,irRef,wire:{dialect:"lekalo-http-wire/v1",contentType:"application/json"},defaults:{errorEnvelope:"canonical-v1",idempotencyHeader:"Idempotency-Key",correlationHeaders:["X-Request-Id"]},securitySchemes:[],endpoints:[{endpoint:"tasks.find_http",params:[{name:"title",in:"query",field:"title",required:true},{name:"tags",in:"query",field:"tags",required:false,style:"form",explode:true}],success:{status:200,body:{mode:"whole-output"}},errorDefaults:{validation:400,auth:403,conflict:409,"not-found":404,domain:422,infrastructure:500}}]};
  assert(schema(transport),JSON.stringify(schema.errors));
  write("transport.json",transport); write("queries.json",query);
  const args=["transport","validate","transport.json","--query-model","queries.json"];
  run([...args,"--strict"]);
  for(const namespace of ["node","laravel","go","rust"]) {
    const projected=run(["transport","project","transport.json","--namespace",namespace,"--query-model","queries.json"]).doc;
    assert.equal(projected.surface.routes.length,1);
  }
  for(const version of ["3.1","3.0"]) {
    const rendered=run(["openapi","render","transport.json","--query-model","queries.json","--version",version]).doc;
    const document=rendered.openapi.document;
    assert(document,JSON.stringify(rendered));
    const parameters=document.paths["/tasks"].get.parameters;
    const title=parameters.find(row=>row.name==="title"), tags=parameters.find(row=>row.name==="tags");
    assert.equal(title.required,true); assert.equal(title.schema.default,"sample");
    assert.equal(tags.required,false); assert.equal(tags.schema.default,null);
    if(version==="3.0") {assert.equal(tags.schema.nullable,true); assert(title.schema.allOf);}
    else assert(tags.schema.type.includes("null"));
    assert.deepEqual(document["x-lekalo-provenance"].irRef,irRef);
    assert.equal(document["x-lekalo-provenance"].transportRef.schemaVersion,transport.schemaVersion);
  }
  run(["transport","validate","transport.json"],1,"query-model-required");
  for(const [member,value,detail] of [["type","tasks.bool","query-input-type"],["required",false,"query-input-required"],["default","other","query-input-default"]]) {
    const invalid=structuredClone(query); invalid.queries[0].parameters[0][member]=value;
    write("queries.json",invalid); run(args,1,detail);
  }
  write("queries.json",query);
  for(const [mutate,detail] of [
    [value=>{value.endpoints[0].params[0].required=false;},"query-input-required"],
    [value=>{value.endpoints[0].params[0].field="input.title";},"field-unresolved"],
    [value=>{value.irRef.digest="sha256:"+"0".repeat(64);},"ir-digest"],
    [value=>{value.modelRef.digest="sha256:"+"0".repeat(64);},"model-digest"],
    [value=>{value.irRef.identity="dev.lekalo.ir@0.2.16";},"ir-identity"],
    [value=>{value.modelRef.modelVersion="0.2.16";},"model-version"],
    [value=>{value.identity="dev.lekalo.transport-http@0.4.0";},"contract-identity"],
  ]) {
    const invalid=structuredClone(transport); mutate(invalid); write("transport.json",invalid); run(args,1,detail);
  }
  write("transport.json",transport);
  write("lekalo/transport.yaml",transport); write("lekalo/query-model.yaml",query);
  watch(run(["validate"]).doc);
  const badHome=structuredClone(query); badHome.queries[0].parameters[0].default="contradiction";
  write("lekalo/query-model.yaml",badHome);
  watch(run(["validate"],1,"query-input-default").doc);
  write("lekalo/query-model.yaml",query);
  assert.deepEqual(files.map(file=>sha(readFileSync(join(stage,file)))),before);
  assert(!existsSync(join(stage,".lekalo")),"read-only signature joins must not create runtime state");
  // A predecessor adapter remains incompatible; schema support must never
  // relabel its native profile as successor generation acceptance.
  mkdirSync(join(stage,"adapters/node-typescript"),{recursive:true});
  cpSync(join(root,"tests/fixtures/orchestration/project/adapters/node-typescript/node-adapter.mjs"),join(stage,"adapters/node-typescript/node-adapter.mjs"));
  const supplied=["--","node","adapters/node-typescript/node-adapter.mjs"];
  run(["lock",...supplied],5,"versioning.adapter-incompatible");
  assert(!existsSync(join(stage,".lekalo/cache/client-sdk")),"an incompatible adapter does not derive SDK evidence");
  assert(!existsSync(join(stage,"src/generated")),"an incompatible adapter must not write source");
  console.log(JSON.stringify({ok:true,gate:"query-transport-bindings",calls,schemas:1,readOnlySourcePreserved:true,generation:"unsupported-adapter-version"}));
} finally {
  assert.equal(dirname(stage),realpathSync.native(tmpdir()));
  assert(basename(stage).startsWith("lekalo-query-transport-"));
  rmSync(stage,{recursive:true,force:true});
}
