#!/usr/bin/env node
// Issue #105: displayed P0 argv replay, not a shell or documentation generator.
import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { ROOT, P0, text, safePath, sha, binary, fences } from "./lib/docs-maintenance.mjs";

const args = process.argv.slice(2);
assert.ok(args.length === 0 || args.join(" ") === "--static" || (args.length === 2 && args[0] === "--lane" && ["portable","planner","mysql"].includes(args[1])), "unknown docs gate arguments");
const lane = args[1] ?? "portable";
const registry = JSON.parse(text("tests/fixtures/docs/examples.json"));
const CHECKS = new Set(["ir","validation","init","inspect","impact","context","contract-update","contract-coverage-missing","contract-check","contract-attach","native-tests","lock","generate-check","verify","trace-validate","trace-export","provider","authority","privacy","provenance","planner-chain","mysql-observed","laravel-vue","typescript","http","mysql"]);
const exact = (object, fields) => assert.deepEqual(Object.keys(object).sort(), [...fields].sort(), "unknown/missing replay metadata field");
function validateRegistry(value) {
  exact(value, ["formatVersion","sourceProduct","setup","examples"]);
  assert.equal(value.formatVersion,1);
  assert.equal(value.sourceProduct,"0.6.3");
  const ids = [...value.setup,...value.examples].map(x=>x.id);
  assert.equal(new Set(ids).size,ids.length,"duplicate block ID");
  for (const row of value.setup) {
    exact(row,["id","document","line","requires"]);
    assert.ok(["built-cli","locked-mysql-dependencies"].includes(row.requires),"unknown setup prerequisite");
    assert.ok(P0.includes(row.document));
  }
  for (const row of value.examples) {
    exact(row,["id","document","fixture","lane","write","commands"]);
    assert.ok(P0.includes(row.document),"unknown example page");
    assert.ok(["portable","planner","mysql"].includes(row.lane),"unknown lane");
    assert.equal(typeof row.write,"boolean");
    if(!["empty","repository"].includes(row.fixture)) assert.ok(statSync(safePath(row.fixture)).isDirectory(),"missing fixture");
    assert.ok(Array.isArray(row.commands) && row.commands.length > 0);
    for(const command of row.commands) {
      exact(command,["line","exit","stream","check"]);
      assert.ok([0,1].includes(command.exit));
      assert.equal(command.stream,command.exit===0?"stdout":"stderr");
      assert.ok(CHECKS.has(command.check),"unknown semantic check");
      assert.match(command.line,/^(lekalo|node) [a-zA-Z0-9_./ :=\-]+$/,"argv must be literal tokens; no shell/interpolation");
    }
  }
}
function validateBlocks(value, read = text) {
  const found = new Set();
  for(const document of P0) {
    for(const block of fences(read(document).replaceAll("\r\n","\n"))) {
      if(["text illustrative","mermaid diagram"].includes(block.info)) {
        if(block.info === "text illustrative") assert.ok(!/^(?:lekalo|node|npm|cargo) /m.test(block.body),"runnable command cannot be illustrative");
        continue;
      }
      const match = /^sh docs-(example|setup)=([a-z0-9-]+)$/.exec(block.info);
      assert.ok(match, `unclassified P0 fence: ${document}`);
      const [,kind,id] = match;
      assert.ok(!found.has(id),"duplicate displayed block");
      found.add(id);
      const row = value[kind==="example"?"examples":"setup"].find(x=>x.id===id);
      assert.ok(row && row.document===document,"unregistered/misplaced block");
      assert.equal(block.body,kind==="example"?row.commands.map(x=>x.line).join("\n"):row.line,"displayed argv drift");
    }
  }
  assert.deepEqual([...found].sort(),[...value.setup,...value.examples].map(x=>x.id).sort(),"orphan replay registry entry");
}
// The linked C/D walkthrough has no local command block, so fence coverage alone
// cannot catch a wrong fixture selection. Its visible working-root links are
// checked against the same recipes that run in contributor order below.
function validatePlannerWalkthrough(value, read = text) {
  const tutorial=read("docs/tutorial-greenfield-planner.md").replaceAll("\r\n","\n");
  let previousHeading=-1, previousRecipe=-1;
  for(const [id,heading,anchor] of [
    ["contract-planner","Contract the planner (C)","contract-one-module"],
    ["inspect-planner","Inspect the planner (D)","projection-example"],
  ]) {
    const index=value.examples.findIndex(row=>row.id===id), row=value.examples[index];
    assert.ok(row && index>previousRecipe,"linked planner replay order drift"); previousRecipe=index;
    const marker=`## ${heading}\n`, position=tutorial.indexOf(marker);
    assert.ok(position>previousHeading && tutorial.split(marker).length===2,"missing/duplicate planner step"); previousHeading=position;
    const section=tutorial.slice(position+marker.length).split(/^## /m)[0];
    const roots=[...section.matchAll(/^Working root: \[([^\]]+)\]\(([^)]+)\)\./gm)];
    assert.equal(roots.length,1,"missing/duplicate planner working root");
    assert.equal(roots[0][1],row.fixture,"linked planner fixture drift");
    assert.equal(roots[0][2],`../${row.fixture}`,"linked planner root URL drift");
    assert.ok(section.includes(`](${row.document.slice(5)}#${anchor})`),"linked planner command page drift");
  }
}
validateRegistry(registry);
validateBlocks(registry);
validatePlannerWalkthrough(registry);
// Scanner declarations are explicit authored inputs, never missing-import
// substitutions. Runtime checking must exclude all of them.
const mysqlFixture="tests/fixtures/pilot/brownfield-mysql";
for(const packageName of ["api","data"]) {
  const config=JSON.parse(text(`${mysqlFixture}/packages/${packageName}/tsconfig.json`));
  for(const paths of Object.values(config.compilerOptions.paths)) for(const path of paths) assert.ok(existsSync(safePath(`${mysqlFixture}/packages/${packageName}/${path}`)),"missing authored scanner declaration");
}
assert.deepEqual(JSON.parse(text(`${mysqlFixture}/tsconfig.runtime.json`)).exclude,["packages/*/types"]);
// Metadata mutation controls exercise the actual validators; they never repair input.
const duplicate = structuredClone(registry); duplicate.examples.push(duplicate.examples[0]);
assert.throws(()=>validateRegistry(duplicate));
const changed = structuredClone(registry); changed.examples[0].commands[0].line += " --invented";
assert.throws(()=>validateBlocks(changed));
assert.throws(()=>validateBlocks(registry,path=>text(path)+(path==="README.md"?"\n\`\`\`sh\nnode unknown.mjs\n\`\`\`\n":"")));
assert.throws(()=>safePath("../fixture-escape"));
assert.throws(()=>validatePlannerWalkthrough(registry,path=>text(path).replaceAll("tests/fixtures/docs/contracted-module","tests/fixtures/contracted/planner-slice")),/linked planner fixture drift/);
assert.throws(()=>validatePlannerWalkthrough(registry,path=>text(path).replaceAll("Working root:","Unregistered root:")),/missing\/duplicate planner working root/);
const reversed=structuredClone(registry);
const contractedIndex=reversed.examples.findIndex(row=>row.id==="contract-planner"), projectionIndex=reversed.examples.findIndex(row=>row.id==="inspect-planner");
[reversed.examples[contractedIndex],reversed.examples[projectionIndex]]=[reversed.examples[projectionIndex],reversed.examples[contractedIndex]];
assert.throws(()=>validatePlannerWalkthrough(reversed),/linked planner replay order drift/);
if(args[0] === "--static") {
  console.log(JSON.stringify({ok:true,gate:"docs-examples",mode:"static",examples:registry.examples.length,setup:registry.setup.length,controls:7}));
  process.exit(0);
}

// A missing built binary or exact schema validator is a refusal, never a skip.
const bin = binary();
const require = createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version,"8.17.1","provision exact Ajv 8.17.1 via NODE_PATH");
const Ajv = require("ajv/dist/2020.js").default;
const ajv = new Ajv({strict:false,allErrors:true});
const schemaCache = new Map();
const schema = (file,value) => {
  if(!schemaCache.has(file)) schemaCache.set(file,ajv.compile(JSON.parse(text(`contracts/${file}`))));
  assert.ok(schemaCache.get(file)(value), `schema refusal: ${file}`);
};
const ignored = new Set([".git","target","node_modules","vendor",".lekalo"]);
const trackedRun=spawnSync("git",["ls-files","-z"],{cwd:ROOT,encoding:"utf8"});
assert.equal(trackedRun.status,0,"tracked fixture custody inventory unavailable");
const trackedHomes=new Set();
for(const path of trackedRun.stdout.split("\0").filter(Boolean)) {
  const parts=path.split("/");
  for(let n=1;n<=parts.length;n++) trackedHomes.add(parts.slice(0,n).join("/"));
}
function admitSource(logical) {
  return !logical.split("/").some(part=>ignored.has(part)) || trackedHomes.has(logical);
}
function inventory(base, logicalBase=relative(ROOT,base).replaceAll("\\","/")) {
  const result = {};
  const walk = path => {
    for(const name of readdirSync(path).sort()) {
      const full=join(path,name);
      const logical=[logicalBase,relative(base,full).replaceAll("\\","/")].filter(Boolean).join("/");
      if(!admitSource(logical)) continue;
      if(statSync(full).isDirectory()) walk(full);
      else result[relative(base,full).replaceAll("\\","/")]=sha(readFileSync(full));
    }
  };
  walk(base);
  return result;
}
const INPUTS=["scripts","adapters","contracts","tests/fixtures","docs","README.md","Cargo.toml","Cargo.lock","crates"];
const sourceBefore = INPUTS.map(path=>statSync(safePath(path)).isDirectory()?inventory(safePath(path)):sha(readFileSync(safePath(path))));
const root=realpathSync.native(mkdtempSync(join(tmpdir(),"lekalo-docs-105-")));
const owned = path => assert.ok(resolve(path).startsWith(root+sep),"temporary path escapes owned replay root");
function copy(from,to) {
  owned(to);
  cpSync(from,to,{recursive:true,filter:path=>admitSource(relative(ROOT,path).replaceAll("\\","/"))});
}
const temp=join(root,"tmp"); mkdirSync(temp);
const childEnv={...process.env,TMPDIR:temp,TEMP:temp,TMP:temp};
// Limit MySQL to the fixture's fixed synthetic database/user and loopback.
if(lane==="mysql") {
  assert.match(childEnv.LEKALO_DOCS_MYSQL_PORT ?? "",/^[0-9]{1,5}$/,"mysql-port-missing");
  assert.ok(Number(childEnv.LEKALO_DOCS_MYSQL_PORT)>0 && Number(childEnv.LEKALO_DOCS_MYSQL_PORT)<=65535);
}
let repository=null;
function repositoryCopy() {
  if(repository) return repository;
  repository=join(root,"repository"); mkdirSync(repository);
  for(const path of INPUTS) copy(safePath(path),join(repository,path));
  const copiedBin=join(repository,"target/debug",basename(bin)); mkdirSync(dirname(copiedBin),{recursive:true}); cpSync(bin,copiedBin);
  if(lane==="planner") {
    const vendor=safePath("tests/fixtures/php-laravel/planner/vendor");
    assert.ok(existsSync(join(vendor,"autoload.php")),"planner-vendor-missing: explicit composer install from lock required");
    cpSync(vendor,join(repository,"tests/fixtures/php-laravel/planner/vendor"),{recursive:true});
    const installed=JSON.parse(readFileSync(join(vendor,"composer/installed.json"),"utf8")).packages;
    const lock=JSON.parse(text("tests/fixtures/php-laravel/planner/composer.lock"));
    for(const pkg of [...lock.packages,...lock["packages-dev"]]) assert.ok(installed.some(x=>x.name===pkg.name && x.version===pkg.version),"Composer lock/installed version mismatch");
    for(const [name,version] of [["typescript","5.9.3"],["@vue/compiler-sfc","3.4.38"]]) assert.equal(require(`${name}/package.json`).version,version,"planner dependency pin mismatch");
  }
  return repository;
}
function mysqlDependencies(destination) {
  const source=safePath("tests/fixtures/pilot/brownfield-mysql");
  const pkg=JSON.parse(readFileSync(join(source,"package.json"),"utf8"));
  const lock=JSON.parse(readFileSync(join(source,"package-lock.json"),"utf8"));
  assert.deepEqual(lock.packages[""].dependencies,pkg.dependencies,"npm lock drift");
  assert.deepEqual(lock.packages[""].devDependencies,pkg.devDependencies,"npm dev lock drift");
  for(const [name,version] of Object.entries({...pkg.dependencies,...pkg.devDependencies})) {
    assert.equal(JSON.parse(readFileSync(join(source,"node_modules",name,"package.json"),"utf8")).version,version,"installed MySQL fixture dependency differs from lock");
  }
  owned(destination);
  cpSync(join(source,"node_modules"),join(destination,"node_modules"),{recursive:true,dereference:true});
}
function run(cwd,line,expected=0,stream="stdout") {
  const [program,...argv]=line.split(" ");
  const executable=program==="lekalo"?bin:process.execPath;
  const result=spawnSync(executable,argv,{cwd,env:childEnv,encoding:"utf8",timeout:900000,maxBuffer:32*1024*1024});
  // Errors print only a stable step token, never raw tool output or paths.
  if(result.status!==expected) {
    const failedLegs=[...(result.stdout ?? "").matchAll(/^FAIL - ([a-z-]+) /gm)].map(x=>x[1]);
    console.error(JSON.stringify({gate:"docs-examples-child",program,exit:result.status,failedLegs}));
  }
  assert.equal(result.status,expected,`unexpected exit: ${program} ${argv.slice(0,2).join(" ")}`);
  assert.equal(result.error,undefined,"child spawn failure");
  if(program==="lekalo") assert.equal(result[stream==="stdout"?"stderr":"stdout"],"","wrong result stream");
  return result[stream] ?? "";
}
function receipt(output) {
  const candidates=[0,...[...output.matchAll(/\n\{/g)].map(x=>x.index+1)];
  for(const i of candidates.reverse()) { try { return JSON.parse(output.slice(i)); } catch {} }
  throw new Error("missing structured receipt");
}
function check(kind,output,cwd) {
  if(kind==="typescript") {assert.equal(output,""); return;}
  if(kind==="http" || kind==="mysql") {assert.match(output,/# pass 1\b/); assert.match(output,/# fail 0\b/); return;}
  if(kind==="native-tests") {assert.match(output,/# pass 2\b/); assert.match(output,/# fail 0\b/); return;}
  if(kind==="authority") {assert.match(output,/authority contract dev\.lekalo\.authority-matrix@0\.3\.2@sha256:[0-9a-f]{64}: PASS/); return;}
  if(kind==="mysql-observed") {
    assert.match(output,/PILOT GREEN .*20 ok \/ 0 failed/);
    const metrics=JSON.parse(readFileSync(join(temp,"lekalo-pilot-brownfield-ts/metrics.json"),"utf8"));
    exact(metrics,["schema","consumer","mode","disposition","steps","totals","projections"]);
    assert.equal(metrics.schema,"lekalo/pilot-brownfield-ts-metrics/v0.1.0");
    assert.equal(metrics.mode,"copy"); assert.equal(metrics.disposition,"public-fixture");
    assert.equal(metrics.totals.failed,0);
    for(const step of ["adopt-dry-run","observe-update","bind-use-case","confirm-handler-binding","inspect","impact","context","baseline","controlled-change"]) assert.equal(metrics.steps.find(x=>x.step===step)?.status,"ok","missing pilot stage");
    const scan=metrics.steps.find(x=>x.step==="scan-wire"); assert.equal(typeof scan.used,"boolean");
    const raw=JSON.stringify(metrics);
    assert.ok(!raw.includes(root) && !raw.includes(ROOT),"raw path in pilot metrics");
    return;
  }
  const value=receipt(output);
  if(kind==="contract-coverage-missing") {
    assert.equal(value.status,"invalid");
    assert.deepEqual(value.reasonCodes,["contracted.coverage-missing","contracted.coverage-missing"]);
    assert.deepEqual(value.diagnostics.map(x=>x.symbol).sort(),["planner.focus_task","planner.list_tasks"]);
    for(const diagnostic of value.diagnostics) {
      schema("diagnostic.schema.v0.2.16.json",diagnostic);
      assert.equal(diagnostic.id,"contracted.coverage-missing"); assert.equal(diagnostic.severity,"error");
      assert.equal(diagnostic.data.detail,"no-native-test");
    }
    return;
  }
  if(kind==="verify") {
    schema("orchestration-report.schema.v0.2.16.json",value);
    assert.equal(value.operation,"verify"); assert.equal(value.verdict,"ready");
    assert.equal(value.components.find(x=>x.id==="model.validation")?.state,"pass");
    for(const component of ["native.gates","scenarios.execution"]) assert.equal(value.components.find(x=>x.id===component)?.state,"unsupported");
    return;
  }
  if(["provenance","planner-chain","laravel-vue"].includes(kind)) {
    assert.equal(value.ok,true);
    if(kind==="provenance") assert.equal(value.evidenceBacked,0);
    if(kind==="planner-chain") {
      assert.equal(value.stages,6);
      assert.deepEqual(value.stageNames,["materialize-shared-planner-project","load-and-validate-project","graph-and-query-projections","semantic-diff-mutation","scenario-lane-compile-and-run","trace-manifest-chain"]);
      assert.ok(value.stageDetails.every(x=>x.ok));
    }
    if(kind==="laravel-vue") {
      assert.ok(value.legs===9 || value.legs===10);
      assert.ok(value.results.every(x=>x.ok));
      for(const leg of ["php-laravel-ui","php-laravel-scenario-tests","php-laravel-parity"]) assert.equal(value.results.find(x=>x.leg===leg)?.ok,true);
    }
    return;
  }
  assert.equal(value.status,"valid");
  if(kind==="ir") {
    exact(value.ir,["contract","definitions","modelVersion","modules","project"]);
    assert.equal(value.ir.contract,"dev.lekalo.ir@0.2.16");
    assert.equal(value.ir.modelVersion,"0.2.16"); assert.equal(value.ir.project.kind,"project");
  } else if(kind==="validation") {
    schema("validation-report.schema.v0.6.3.json",value);
    assert.equal(value.validation.profile,"strict"); assert.equal(value.validation.counts.error,0);
  } else if(kind==="inspect") {
    schema("inspect.schema.v0.2.16.json",value.inspect);
    assert.equal(value.inspect.selector.resolvedId,"planner.focus_task");
    assert.equal(value.inspect.symbol.kind,"command");
  } else if(kind==="impact") {
    schema("impact.schema.v0.2.16.json",value.impact);
    assert.deepEqual(value.impact.roots,["operation:planner.focus_task"]);
    assert.ok(value.impact.direct.items.some(x=>x.subject==="endpoint:planner.endpoint_focus_task"));
  } else if(kind==="context") {
    schema("context-capsule.schema.v0.2.16.json",value.context);
    assert.equal(value.context.budget.limit,5000); assert.equal(value.context.budget.fits,true);
    assert.ok(value.context.manifest.included.length>0);
  } else if(kind==="trace-validate") {
    assert.equal(value.trace.manifestId,"planner-trace-full"); assert.equal(value.trace.nodeCount,9); assert.equal(value.trace.relationCount,9);
  } else if(kind==="trace-export") {
    schema("trace-manifest.schema.v0.2.16.json",value.trace);
    assert.equal(value.trace.completeness,"full"); assert.equal(value.trace.nodes.length,9);
    assert.equal(value.manifestDigest,sha(JSON.stringify(value.trace)));
  } else if(kind==="provider") {
    schema("provider-capabilities.schema.v0.6.3.json",value);
  } else if(kind==="init") {
    assert.ok(existsSync(join(cwd,"lekalo/project.yaml"))); assert.ok(existsSync(join(cwd,"lekalo/modules/planner/module.yaml")));
  } else if(kind==="contract-update") {
    assert.equal(value.symbols,4); assert.ok(value.recorded.includes("planner.focus_task"));
  } else if(kind==="contract-attach" || kind==="contract-check") {
    assert.ok(!value.diagnostics?.some(x=>x.severity==="error"));
  } else if(kind==="privacy") {
    assert.equal(value.policyRef.version,"0.3.2"); assert.equal(value.accepted,true);
  } else if(kind==="lock") {
    assert.equal(value.counts.adapters,0); schema("lock.schema.v0.3.2.json",JSON.parse(readFileSync(join(cwd,"lekalo.lock"),"utf8")));
  } else if(kind==="generate-check") {
    schema("generate-check-receipt.schema.v0.6.3.json",value);
    assert.equal(value.verdict,"clean"); assert.equal(value.counts.artifacts,0);
  } else throw new Error("unhandled semantic check");
}
let commands=0, controls=7;
const completed=[];
try {
  for(const row of registry.examples.filter(x=>x.lane===lane)) {
    let cwd;
    if(row.fixture==="repository") cwd=repositoryCopy();
    else {
      cwd=join(root,row.id); mkdirSync(cwd);
      if(row.fixture!=="empty") copy(safePath(row.fixture),cwd);
      if(lane==="mysql") mysqlDependencies(cwd);
    }
    const logicalBase=row.fixture==="repository"?"":row.fixture==="empty"?"empty":row.fixture;
    const before=inventory(cwd,logicalBase);
    for(const command of row.commands) {
      const output=run(cwd,command.line,command.exit,command.stream);
      check(command.check,output,cwd); commands++;
      if(command.line.startsWith("lekalo ") && !row.write) {
        assert.equal(run(cwd,command.line),output,"non-deterministic read-only output"); controls++;
      }
    }
    // Every pre-existing source byte remains unchanged, including maintained code.
    const afterSource=inventory(cwd,logicalBase);
    for(const [path,digest] of Object.entries(before)) assert.equal(afterSource[path],digest,"tracked fixture input changed");
    if(!row.write) assert.deepEqual(afterSource,before,"unexpected fixture source write");
    if(!row.write) {
      // Runtime files are ignored by the source inventory, but direct read-only
      // CLI recipes must not create even a runtime home.
      if(row.fixture!=="repository") assert.ok(!existsSync(join(cwd,".lekalo")),"read-only runtime write");
    }
    if(row.id==="minimal-init") {
      const after=inventory(cwd,logicalBase);
      run(cwd,row.commands[0].line);
      assert.deepEqual(inventory(cwd,logicalBase),after,"init idempotence failed"); controls++;
      const file=join(cwd,"lekalo/project.yaml");
      const original=readFileSync(file);
      writeFileSync(file,"unrelated-conflicting-content\n");
      const denied=receipt(run(cwd,row.commands[0].line,3));
      assert.equal(denied.status,"denied");
      assert.equal(readFileSync(file,"utf8"),"unrelated-conflicting-content\n"); controls++;
      writeFileSync(file,original);
    }
    if(row.id==="inspect-planner") {
      const small=receipt(run(cwd,"lekalo --no-cache --json context planner.focus_task --budget 100"));
      schema("context-capsule.schema.v0.2.16.json",small.context);
      assert.equal(small.context.budget.fits,false); assert.ok(small.context.manifest.excluded.length>0); controls++;
      const refused=receipt(run(cwd,"lekalo --no-cache --json inspect planner.nonexistent",1,"stderr"));
      assert.equal(refused.status,"invalid");
      assert.ok(refused.diagnostics.some(x=>x.id.includes("unknown"))); controls++;
    }
    if(row.id==="read-minimal") {
      const file=join(cwd,"lekalo/project.yaml"); writeFileSync(file,"{ malformed");
      const beforeBad=inventory(cwd,logicalBase);
      const invalid=receipt(run(cwd,row.commands[0].line,1,"stderr"));
      assert.equal(invalid.status,"invalid"); assert.ok(invalid.reasonCodes.includes("loader.json-parse"));
      assert.deepEqual(inventory(cwd,logicalBase),beforeBad); controls++;
    }
    if(row.id==="contract-planner") {
      const file=join(cwd,"src/focus.ts"); writeFileSync(file,readFileSync(file,"utf8")+"\n// controlled docs drift\n");
      const invalid=receipt(run(cwd,row.commands.at(-1).line,1,"stderr"));
      assert.equal(invalid.status,"invalid");
      assert.ok(invalid.diagnostics.some(x=>x.id==="contracted.binding-drift" && x.symbol==="planner.focus_task" && /fingerprint|content/.test(x.data.detail))); controls++;
    }
    completed.push(row.id);
    console.log(`ok - docs ${row.id}`);
  }
  assert.ok(completed.length>0,"empty required lane");
  const sourceAfter=INPUTS.map(path=>statSync(safePath(path)).isDirectory()?inventory(safePath(path)):sha(readFileSync(safePath(path))));
  assert.deepEqual(sourceAfter,sourceBefore,"repository source preservation failed");
  console.log(JSON.stringify({ok:true,gate:"docs-examples",lane,examples:completed.length,commands,controls,sourcePreserved:true}));
} finally {
  assert.equal(dirname(root),realpathSync.native(tmpdir()),"temporary cleanup target changed");
  assert.ok(basename(root).startsWith("lekalo-docs-105-"));
  rmSync(root,{recursive:true,force:true});
}
