#!/usr/bin/env node
// Issue #72 client-SDK generator gate: the generation composite claims
// the `generate.client-sdk` capability, the render over the committed
// SDK evidence is byte-stable across repeats, the write plan carries
// the client module plus the compatibility and ownership sidecars, the
// apply publishes exactly the declared plan, and the verify posture
// recomputes without drift and reports a maintained edit. Driven
// through the production kernel. The successor native leg requires the
// adapter's pinned TypeScript compiler and an installed Go toolchain.
import assert from "node:assert/strict";
import { test } from "node:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";

import { createKernel, validateResolvedProjectProfile } from "../adapters/node-typescript/src/kernel.mjs";
import { descriptor as generationComposite } from "../adapters/node-typescript/src/generation-composite.mjs";
import { CLIENT_SDK_CAPABILITY, clientSdkGenerateOperation } from "../adapters/node-typescript/src/client-sdk-gen.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const goldenBytes = readFileSync(
  join(repoRoot, "tests/fixtures/client-sdk/golden/planner.expect.json"),
  "utf8",
);

const PROFILE = {
  id: "standalone",
  mode: "observed",
  target: "node-typescript",
  readRoots: [
    { kind: "tree", path: ".lekalo/cache/client-sdk" },
    { kind: "tree", path: "src" },
  ],
  exclusions: [],
  provenance: { origin: "declared", revision: "test", disposition: "public-fixture" },
};

const REQUEST = {
  protocol: "lekalo.target/v1",
  protocol_version: "0.3.2",
  operation: "generate",
  request_id: "req-client-sdk-gate-1",
  project_root: ".",
  target: "node-typescript",
  profile: "standalone",
  dry_run: true,
  ir_path: ".lekalo/cache/client-sdk/planner.json",
};

function evidenceProject() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-client-sdk-gate-")));
  const sdkDir = join(root, ".lekalo", "cache", "client-sdk");
  mkdirSync(sdkDir, { recursive: true });
  writeFileSync(join(sdkDir, "planner.json"), goldenBytes, "utf8");
  // The declared `src` read root must physically exist at kernel
  // creation.
  mkdirSync(join(root, "src"), { recursive: true });
  return root;
}

function bareProject() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-client-sdk-gate-")));
  mkdirSync(join(root, ".lekalo", "cache", "client-sdk"), { recursive: true });
  mkdirSync(join(root, "src"), { recursive: true });
  return root;
}

function kernelFor() {
  return createKernel({
    resolvedProjectProfile: validateResolvedProjectProfile({ ...PROFILE }),
    extensionRegistry: [generationComposite],
  });
}

function dispatch(kernel, root, request) {
  const { response } = kernel.dispatch(request, { permittedProjectRoot: root });
  return response;
}

test("gate: the composite claims the client-SDK capability honestly", () => {
  assert.equal(generationComposite.namedCapabilities[CLIENT_SDK_CAPABILITY], "partial");
  assert.ok(
    generationComposite.writeScopes.includes("src/generated/node-typescript/clients/**"),
  );
});

test("gate: the dry-run plan is deterministic across repeats", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const first = dispatch(kernel, root, REQUEST);
    assert.equal(first.status, "ok", JSON.stringify(first.error ?? {}));
    const writes = first.writes ?? [];
    assert.ok(writes.length >= 3, "the client module plus the sidecars");
    for (let index = 0; index < 2; index += 1) {
      const repeat = dispatch(kernel, root, {
        ...REQUEST,
        request_id: `req-client-sdk-gate-repeat-${index}`,
      });
      assert.equal(repeat.status, "ok");
      assert.deepEqual(repeat.writes, writes, "generation is deterministic");
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("gate: the plan carries the client module and the sidecars", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const response = dispatch(kernel, root, REQUEST);
    const writes = response.writes ?? [];
    const paths = writes.map((write) => write.path).sort();
    assert.ok(
      paths.some((path) => path.endsWith("planner.client.ts")),
      `client module in plan: ${JSON.stringify(paths)}`,
    );
    assert.ok(
      paths.some((path) => path.endsWith("planner.client.go")),
      `the Go backend module is in the plan: ${JSON.stringify(paths)}`,
    );
    assert.ok(paths.some((path) => path.endsWith("planner.compatibility.json")));
    assert.ok(paths.some((path) => path.endsWith("planner.map.json")));
    // Every write stays inside the declared client write root.
    for (const path of paths) {
      assert.ok(path.startsWith("src/generated/node-typescript/clients/"), path);
    }
    // The plan is the dry-run authority: no file exists yet.
    assert.ok(
      !existsSync(join(root, "src/generated/node-typescript/clients/planner.client.ts")),
      "a dry run writes nothing",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("gate: apply publishes the plan and verify recomputes without drift", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const plan = dispatch(kernel, root, REQUEST);
    assert.equal(plan.status, "ok", JSON.stringify(plan.error ?? {}));
    const apply = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-client-sdk-gate-apply",
      dry_run: false,
      plan_id: plan.evidence?.plan_id,
    });
    assert.equal(apply.status, "ok", JSON.stringify(apply.error ?? {}));
    const clientPath = join(root, "src/generated/node-typescript/clients/planner.client.ts");
    assert.ok(existsSync(clientPath), "the applied client module exists");
    // The emitted module injects transport and base URL; it never
    // hard-codes either.
    const text = readFileSync(clientPath, "utf8");
    assert.match(text, /export interface LekaloTransport/);
    assert.match(text, /export class LekaloClient/);
    assert.ok(!text.includes("http://") && !text.includes("https://"), "no hardcoded URL");
    // The applied compatibility sidecar binds the exact evidence
    // digest and never carries a timestamp or a path.
    const compatibility = JSON.parse(
      readFileSync(
        join(root, "src/generated/node-typescript/clients/planner.compatibility.json"),
        "utf8",
      ),
    );
    assert.equal(compatibility.projectId, "planner");
    assert.match(compatibility.sdkContract.digest, /^sha256:[0-9a-f]{64}$/);
    assert.ok(!/\d{4}-\d{2}-\d{2}T/.test(JSON.stringify(compatibility)), "no timestamps");
    // Verify recomputes without drift.
    const verify = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-client-sdk-gate-verify",
      operation: "verify",
    });
    assert.equal(verify.status, "ok", JSON.stringify(verify.error ?? {}));
    const findings = verify.result?.findings ?? verify.findings ?? [];
    assert.deepEqual(
      findings.filter((finding) => finding.code === "client.drift"),
      [],
      "no drift over the applied bytes",
    );
    // A maintained edit drifts.
    const maintained = readFileSync(clientPath, "utf8").replace(
      "never edit",
      "maintained edit",
    );
    writeFileSync(clientPath, maintained, "utf8");
    const drifted = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-client-sdk-gate-drift",
      operation: "verify",
    });
    const driftFindings = (drifted.result?.findings ?? drifted.findings ?? []).filter(
      (finding) => finding.code === "client.drift",
    );
    assert.ok(driftFindings.length > 0, "the maintained edit drifts");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("gate: absent SDK evidence fails honestly when the profile reads the home", () => {
  const root = bareProject();
  try {
    const kernel = kernelFor();
    const response = dispatch(kernel, root, REQUEST);
    assert.equal(response.status, "error");
    // The failure is in-envelope and named: the SDK home is readable
    // but the evidence file is absent (or the sibling IR path is), and
    // every owner refuses explicitly instead of planning an empty
    // client.
    assert.match(
      JSON.stringify(response.error),
      /client-sdk-evidence-absent|ir-unreadable|ir-evidence-absent/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("successor: exact pins, optional empty values and explicit native parameter boundaries", async () => {
  const root=evidenceProject();
  try {
    const document=JSON.parse(goldenBytes);
    document.schemaVersion="lekalo/client-sdk/v0.8.0";
    document.identity="dev.lekalo.client-sdk@0.8.0";
    document.modelRef.modelVersion="0.8.0";
    document.irRef.identity="dev.lekalo.ir@0.8.0";
    document.transportRef.identity="dev.lekalo.transport-http@0.8.0";
    // Keep this native profile's response/query surface bounded to one
    // operation; the core integration test separately checks typed lists.
    document.operations=[document.operations.find(row=>row.ident==="listTasks")];
    const operation=document.operations[0];
    delete operation.pagination; delete operation.correlation; delete operation.auth;
    operation.params=[
      {name:"optional",in:"query",field:"optional",typeRef:"planner.text",shape:"value",nullable:false,required:false},
      {name:"title",in:"query",field:"title",typeRef:"planner.text",shape:"value",nullable:false,required:true},
    ];
    const evidence=join(root,".lekalo/cache/client-sdk/planner.json");
    writeFileSync(evidence,JSON.stringify(document));
    const kernel=kernelFor();
    const plan=dispatch(kernel,root,REQUEST);
    assert.equal(plan.status,"ok",JSON.stringify(plan.error));
    const apply=dispatch(kernel,root,{...REQUEST,dry_run:false,plan_id:plan.evidence?.plan_id});
    assert.equal(apply.status,"ok",JSON.stringify(apply.error));
    const home=join(root,"src/generated/node-typescript/clients");
    const typescript=readFileSync(join(home,"planner.client.ts"),"utf8");
    assert.match(typescript,/Contract: lekalo\/client-sdk\/v0\.8\.0/);
    assert.match(typescript,/listTasks\(title: string, optional\?: string\)/);
    const go=readFileSync(join(home,"planner.client.go"),"utf8");
    assert.match(go,/optional \*string/);
    assert.match(go,/if optional != nil/);
    assert.match(go,/query.Set\("optional", \*optional\)/);
    // Typecheck the generated module with the same pinned compiler bundled
    // into the production adapter, then execute its actual call method.
    const require=createRequire(import.meta.url);
    const ts=require("../adapters/node-typescript/node_modules/typescript");
    assert.equal(ts.version,"5.9.3");
    const filename=join(home,"planner.client.ts");
    const program=ts.createProgram([filename],{strict:true,target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext,noEmit:true,types:[]});
    assert.deepEqual(ts.getPreEmitDiagnostics(program).map(row=>ts.flattenDiagnosticMessageText(row.messageText,"\n")),[]);
    const emitted=ts.transpileModule(typescript,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText;
    const {LekaloClient}=await import("data:text/javascript;base64,"+Buffer.from(emitted).toString("base64"));
    const sent=[];
    const client=new LekaloClient({baseUrl:"",transport:{send:async request=>{sent.push(request);return {status:200,body:"[]"};}}});
    await client.listTasks("title");
    await client.listTasks("title","");
    assert(!Object.hasOwn(sent[0].query,"optional"));
    assert.equal(sent[1].query.optional,"");
    assert.equal(sent[1].query.title,"title");
    writeFileSync(join(home,"go.mod"),"module sdk-presence-test\n\ngo 1.20\n");
    writeFileSync(join(home,"presence_test.go"),`package planner
import ("context"; "testing")
type recordedTransport struct { requests []Request }
func (transport *recordedTransport) Send(ctx context.Context, request Request) (Response, error) {
  transport.requests = append(transport.requests, request)
  return Response{Status:200,Body:[]byte("[]")}, nil
}
func TestOptionalEmptyPresence(t *testing.T) {
  transport := &recordedTransport{}
  client := NewPlannerClient("", transport, nil)
  if _, err := client.ListTasks(context.Background(), nil, "title"); err != nil { t.Fatal(err) }
  empty := ""
  if _, err := client.ListTasks(context.Background(), &empty, "title"); err != nil { t.Fatal(err) }
  if _, ok := transport.requests[0].Query["optional"]; ok { t.Fatal("absent optional parameter serialized") }
  values, ok := transport.requests[1].Query["optional"]
  if !ok || len(values) != 1 || values[0] != "" { t.Fatal("present empty parameter erased") }
}
`);
    const goTest=spawnSync("go",["test","-count=1","./..."],{cwd:home,encoding:"utf8",timeout:120000,env:{...process.env,GOTOOLCHAIN:"local",GOPROXY:"off",GOSUMDB:"off",GOWORK:"off"}});
    assert.equal(goTest.error,undefined);
    assert.equal(goTest.status,0,goTest.stderr||goTest.stdout);
    const mixed=structuredClone(document); mixed.transportRef.identity="dev.lekalo.transport-http@0.4.0";
    writeFileSync(evidence,JSON.stringify(mixed));
    const refused=dispatch(kernel,root,REQUEST);
    assert.equal(refused.status,"error");
    for(const override of [{shape:"list"},{nullable:true},{in:"cookie"}]) {
      const unsupported=structuredClone(document); Object.assign(unsupported.operations[0].params[0],override);
      writeFileSync(evidence,JSON.stringify(unsupported));
      const response=dispatch(kernel,root,REQUEST);
      assert.notEqual(response.status,"ok");
      assert.equal(response.error.class,"unsupported");
      const owner=clientSdkGenerateOperation({request:REQUEST,readView:{canRead:path=>path===REQUEST.ir_path,readFile:()=>Buffer.from(JSON.stringify(unsupported))}});
      assert.equal(owner.state,"unsupported");
      assert.deepEqual(owner.diagnostics,[{reason:"client-sdk-parameter-unsupported"}]);
    }
  } finally {
    rmSync(root,{recursive:true,force:true});
  }
});
