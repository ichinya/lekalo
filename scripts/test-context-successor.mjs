#!/usr/bin/env node
// Real CLI and official MCP host: successor facts stay typed and source-pinned.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { basename, dirname, join, resolve } from "node:path";
import { cpSync, existsSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const require=createRequire(import.meta.url);
const schemaRequire=process.env.LEKALO_AJV_NODE_PATH?createRequire(join(process.env.LEKALO_AJV_NODE_PATH,"package.json")):require;
assert.equal(schemaRequire("ajv/package.json").version,"8.20.0");
const Ajv=schemaRequire("ajv/dist/2020").default;
const {Client}=require("@modelcontextprotocol/sdk/client/index.js");
const {StdioClientTransport}=require("@modelcontextprotocol/sdk/client/stdio.js");
const root=resolve(dirname(fileURLToPath(import.meta.url)),"..");
const bin=process.env.LEKALO_BIN??join(root,"target/debug",process.platform==="win32"?"lekalo.exe":"lekalo");
assert(existsSync(bin),"build the checkout CLI first");
const ajv=new Ajv({strict:true,allErrors:true});
const schema=name=>ajv.compile(JSON.parse(readFileSync(join(root,`contracts/${name}.schema.v0.8.0.json`),"utf8")));
const contextSchema=schema("context-capsule"), contextOutput=schema("mcp-tool-context-output"), graphOutput=schema("mcp-tool-graph-output");
const resultSchema=schema("context-result");
const validate=(validator,value)=>assert(validator(value),JSON.stringify(validator.errors?.slice(-8)));
const stage=realpathSync.native(mkdtempSync(join(tmpdir(),"lekalo-context-successor-")));
const sha=bytes=>createHash("sha256").update(bytes).digest("hex");
let calls=0;
function run(args,json=true) {
  calls++;
  const result=spawnSync(bin,[...args,"--project",".","--no-cache",...(json?["--json"]:[])],{cwd:stage,encoding:"utf8",timeout:30000,env:{...process.env,LEKALO_PROJECT:""}});
  assert.equal(result.error,undefined); assert.equal(result.status,0,result.stderr||result.stdout);
  return json?{wire:result.stdout.trimEnd(),doc:JSON.parse(result.stdout)}:result.stdout;
}
function card(capsule,id) {
  return Object.values(capsule.sections).flat().find(fact=>fact.id===`operation:${id}`||fact.id===`entity:${id}`||fact.id===`type:${id}`);
}
try {
  cpSync(join(root,"tests/fixtures/change/field-vertical"),stage,{recursive:true,filter:path=>basename(path)!==".lekalo"});
  const files=["lekalo/project.yaml","lekalo/modules/tasks/module.yaml","lekalo/modules/tasks/entities.yaml"];
  for(const file of files)writeFileSync(join(stage,file),readFileSync(join(stage,file),"utf8").replaceAll("0.2.16","0.8.0"));
  const write=(file,source)=>{files.push(file);writeFileSync(join(stage,file),source);};
  write("lekalo/modules/tasks/commands.yaml",`schema_version: '0.8.0'
definitions:
  - id: tasks.complete
    kind: command
    version: 1
    input:
      - name: title
        type: tasks.text
        required: true
        default: sample
    returns: 'list<tasks.task>?'
    emits:
      - tasks.completed
`);
  write("lekalo/modules/tasks/queries.yaml",`schema_version: '0.8.0'
definitions:
  - id: tasks.find
    kind: query
    version: 1
    input:
      - name: tags
        type: 'list<tasks.text?>?'
        default: null
      - name: maximum
        type: tasks.integer
        default: 9223372036854775807
    reads:
      - tasks.task
    returns: tasks.task
  - id: tasks.empty
    kind: query
    version: 1
    reads:
      - tasks.task
    returns: tasks.task
`);
  write("lekalo/modules/tasks/events.yaml",`schema_version: '0.8.0'
definitions:
  - id: tasks.completed
    kind: event
    version: 1
    payload:
      - name: title
        type: tasks.text
        default: event-title
`);
  const entities=join(stage,"lekalo/modules/tasks/entities.yaml");
  writeFileSync(entities,readFileSync(entities,"utf8").replace("      - value: low","      - value: low\n        wire_value: 17")+`    relations:
      - id: tasks.rel.parent
        target: tasks.task
        localFields:
          - title
        targetFields:
          - title
        min: 0
        max: null
        ownership: local
        onDetach: retain-history
        onDelete: restrict
    uniqueConstraints:
      - id: tasks.unique.title
        fields:
          - title
        comparison: exact-text
        scope: global
  - id: tasks.integer
    kind: scalar
    version: 1
    base: number
`);
  const before=files.map(file=>sha(readFileSync(join(stage,file))));
  const symbols=["tasks.complete","tasks.find","tasks.empty","tasks.task","tasks.priority","tasks.completed"];
  const capsules=new Map();
  for(const symbol of symbols) {
    const args=["context",symbol,"--budget","100000"];
    const first=run(args);validate(contextSchema,first.doc.context);validate(resultSchema,first.doc);
    assert.equal(first.wire,run(args).wire,"deterministic JSON");
    const markdown=run(args,false);assert.equal(markdown,run(args,false),"deterministic Markdown");
    assert(markdown.includes("dev.lekalo.context@0.8.0"));
    if(symbol==="tasks.find") assert(markdown.includes("9223372036854775807")&&markdown.includes("default input tags: null"),markdown);
    if(symbol==="tasks.complete") assert(markdown.includes("returns: list<tasks.task>?")&&markdown.includes("emits: tasks.completed"),markdown);
    if(symbol==="tasks.task") assert(markdown.includes("tasks.rel.parent")&&markdown.includes("tasks.unique.title"),markdown);
    assert.equal(first.doc.context.modelVersion,"0.8.0");
    capsules.set(symbol,first);
  }
  const command=card(capsules.get("tasks.complete").doc.context,"tasks.complete");
  assert(command,JSON.stringify(capsules.get("tasks.complete").doc.context.sections));
  assert.deepEqual(command.contract.emits,["tasks.completed"]);
  assert.equal(command.contract.input[0].default,"sample");
  assert.deepEqual(command.contract.returns,{optional:{list:{ref:"tasks.task"}}});
  assert(capsules.get("tasks.complete").doc.context.sections.effects.some(fact=>fact.kind==="emit-event"&&fact.resource.id==="tasks.completed"));
  const query=card(capsules.get("tasks.find").doc.context,"tasks.find");
  const tags=query.contract.input.find(field=>field.name==="tags");
  assert.deepEqual(tags.type,{optional:{list:{optional:{ref:"tasks.text"}}}});
  assert.equal(tags.default,null);
  assert(capsules.get("tasks.find").wire.includes('"default":9223372036854775807'));
  assert.deepEqual(card(capsules.get("tasks.empty").doc.context,"tasks.empty").contract.input,[]);
  const entity=card(capsules.get("tasks.task").doc.context,"tasks.task");
  assert.equal(entity.contract.relations[0].id,"tasks.rel.parent");
  assert.equal(entity.contract.uniqueConstraints[0].id,"tasks.unique.title");
  assert.equal(card(capsules.get("tasks.priority").doc.context,"tasks.priority").contract.values.find(value=>value.value==="low").wireValue,17);
  const tiny=run(["context","tasks.find","--budget","1"]).doc.context;
  validate(contextSchema,tiny);assert.equal(tiny.budget.fits,false);
  assert.deepEqual(tiny.sections,{});
  assert(tiny.manifest.excluded.some(row=>row.id==="operation:tasks.find"&&row.reason==="budget"));
  assert.equal(tiny.budget.minimumRequired,capsules.get("tasks.find").doc.context.budget.minimumRequired);
  const rootTokens=capsules.get("tasks.find").doc.context.manifest.included.find(row=>row.id==="operation:tasks.find").tokens;
  const rootOnly=run(["context","tasks.find","--budget",String(rootTokens)]).doc.context;
  validate(contextSchema,rootOnly);
  assert.deepEqual(card(rootOnly,"tasks.find").contract,query.contract,"a fact stays whole when its recorded budget fits");

  writeFileSync(join(stage,"mcp.local.json"),JSON.stringify({schemaVersion:"lekalo/mcp-server-config/v0.8.0",identity:"dev.lekalo.mcp-server-config@0.8.0",projects:[{id:"tasks",root:"."}]}));
  const transport=new StdioClientTransport({command:bin,args:["mcp","serve","--stdio","--catalog-config","mcp.local.json"],cwd:stage,env:{...process.env,LEKALO_PROJECT:""}});
  const client=new Client({name:"lekalo-context-successor-gate",version:"0.8.0"},{capabilities:{}});
  try {
    await client.connect(transport);
    for(const symbol of symbols) {
      const response=await client.callTool({name:"lekalo_context",arguments:{projectId:"tasks",mode:"symbol",symbol,budget:100000}});calls++;
      assert.equal(response.isError,false,JSON.stringify(response));
      const envelope=response.structuredContent;validate(contextOutput,envelope);
      assert.equal(envelope.contractPins.context,"0.8.0");
      assert.deepEqual(envelope.result.context,capsules.get(symbol).doc.context);
      const wrong=structuredClone(envelope);wrong.contractPins.context="0.2.16";assert.equal(contextOutput(wrong),false);
    }
    const changed=await client.callTool({name:"lekalo_context",arguments:{projectId:"tasks",mode:"changed",changedSymbols:["tasks.complete","tasks.find"],budget:100000}});calls++;
    assert.equal(changed.isError,false);validate(contextOutput,changed.structuredContent);
    const bounded=await client.callTool({name:"lekalo_context",arguments:{projectId:"tasks",mode:"symbol",symbol:"tasks.find",budget:1}});calls++;
    assert.equal(bounded.isError,false);validate(contextOutput,bounded.structuredContent);
    assert.deepEqual(bounded.structuredContent.result.context,tiny);
    for(const mode of ["show","callers","path","export"]) {
      const args={projectId:"tasks",mode,...(mode==="path"?{from:"tasks.complete",to:"tasks.completed"}:mode==="export"?{}:{symbol:"tasks.completed"})};
      const response=await client.callTool({name:"lekalo_graph",arguments:args});calls++;
      assert.equal(response.isError,false,JSON.stringify(response));
      const envelope=response.structuredContent;validate(graphOutput,envelope);
      assert.equal(envelope.contractPins.graph,"0.8.0");
      if(mode==="export") {const wrong=structuredClone(envelope);wrong.contractPins.graph="0.2.16";assert.equal(graphOutput(wrong),false);}
    }
  } finally {await client.close();}
  const commandPath=join(stage,"lekalo/modules/tasks/commands.yaml"), commandBefore=readFileSync(commandPath);
  try {
    writeFileSync(commandPath,commandBefore.toString("utf8").replace("default: sample",`default: ${"x".repeat(2000)}`));
    const enlarged=run(["context","tasks.complete","--budget","100000"]).doc.context;
    assert(enlarged.budget.minimumRequired>capsules.get("tasks.complete").doc.context.budget.minimumRequired,"literal bytes count towards the recorded budget");
  } finally {writeFileSync(commandPath,commandBefore);}
  assert.deepEqual(files.map(file=>sha(readFileSync(join(stage,file)))),before);
  assert(!existsSync(join(stage,".lekalo")),"context/MCP reads must not write runtime state");
  console.log(JSON.stringify({ok:true,gate:"context-successor",calls,schemas:4,officialMcpHost:true,readOnlySourcePreserved:true}));
} finally {
  assert.equal(dirname(stage),realpathSync.native(tmpdir()));assert(basename(stage).startsWith("lekalo-context-successor-"));
  rmSync(stage,{recursive:true,force:true,maxRetries:5});
}
