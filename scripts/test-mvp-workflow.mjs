#!/usr/bin/env node
// Clean synthetic user workflow through the released CLI and untouched adapter.
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import {join} from 'node:path';
import {repo,binary,catalog,cli,prepare,verify,save,hash,bytes,inventory,cleanup} from './test-node-native-tools.mjs';

const fixture=join(repo,'tests/fixtures/public-typescript-scan');
const scope={schemaVersion:'lekalo/discovery-scope/v0.9.0',identity:'dev.lekalo.discovery-scope@0.9.0',includes:['.'],excludes:['packages/web/out'],gitIgnore:'respect'};
const results=[];
let original;
try {
  const f=prepare('mvp',{
    adoptScope:scope,
    mutate(root){
      fs.writeFileSync(join(root,'packages/core/src/score.ts'),'export class score { constructor(public value: string) {} }\nexport function set_value(current: score, value: string): score { current.value = value; return current; }\n');
      fs.writeFileSync(join(root,'packages/web/src/preview.ts'),'import {score} from "../../core/src/score.ts";\nexport function preview(value: string): string { return new score(value).value; }\n');
      fs.writeFileSync(join(root,'packages/core/test/score.test.ts'),'import {expect,test} from "vitest";\nimport {score,set_value} from "../src/score.ts";\ntest("declared local state update",()=>{const state=new score("before");expect(set_value(state,"after").value).toBe("after");expect(state.value).toBe("after");});\n');
      fs.mkdirSync(join(root,'packages/web/out'),{recursive:true});
      fs.writeFileSync(join(root,'packages/web/out/excluded.ts'),'this is deliberately invalid generated output;\n');
      save(join(root,'tsconfig.json'),{compilerOptions:{target:'ES2022',module:'ESNext',moduleResolution:'Bundler',strict:true,noEmit:true,allowImportingTsExtensions:true},include:['packages/core/src/**/*.ts','packages/web/src/preview.ts']});
      original=inventory(root);
    },
    afterAdopt(root,home){
      for(const name of ['native_core','native_web'])if(!fs.existsSync(join(root,'lekalo/modules',name)))cli(root,['module','new',name]);
      for(const [from,to]of [['model/entities.yaml','entities.yaml'],['domain/commands.yaml','commands.yaml']]){
        const text=fs.readFileSync(join(fixture,from),'utf8').replaceAll('public_core','native_core');
        fs.writeFileSync(join(root,'lekalo/modules/native_core',to),text);
      }
      const denied=cli(root,['validate','--strict'],3);
      assert.ok(denied.reasonCodes.includes('authorization.effect-unprotected'));
      const model=cli(root,['load','--ir']);
      const authorization=JSON.parse(fs.readFileSync(join(fixture,'domain/authorization.json'),'utf8').replaceAll('public_core','native_core'));
      authorization.model_ref.ir_digest=hash(JSON.stringify(model.ir));
      save(join(root,'lekalo/authorization.yaml'),authorization);
      cli(root,['validate','--strict']);
      const profile={id:'standalone',mode:'observed',target:'node-typescript',readRoots:[{path:'package.json',kind:'file'},{path:'tsconfig.json',kind:'file'},{path:'packages',kind:'tree'}],exclusions:['packages/core/test/**','packages/web/src/main.ts','packages/web/out/**'],provenance:{origin:'declared',revision:'mvp-domain-client-1',disposition:'user-confirmed'}};
      const scanned=cli(root,['scan','--target','node-typescript','--profile','standalone','--project','.',process.execPath,join(repo,'adapters/node-typescript/adapter.mjs'),'--lekalo-project-profile-json',JSON.stringify(profile)]);
      assert.ok(scanned.symbols>=2);
      const proposals=cli(root,['bindings','propose']);
      let confirmed=0;
      for(const proposal of proposals.proposals){
        assert.equal(proposal.ambiguous,false);
        cli(root,['bindings','confirm',proposal.proposal]);confirmed++;
      }
      assert.ok(confirmed>=2);
      cli(root,['bindings','audit']);
      const observed=cli(root,['observe','inspect','native_core.score']);
      assert.equal(observed.binding,'confirmed');
      cli(root,['validate','--strict']);
      cli(root,['inspect','native_core.set_value']);
      cli(root,['impact','native_core.score']);
      const context=cli(root,['context','native_core.set_value','--budget','4000']);
      cli(root,['install','prompt','--create-agents-md']);
      cli(root,['install','prompt','--check']);
      const ir=JSON.stringify(cli(root,['load','--ir']));
      assert.equal(ir.includes('"kind":"http"'),false);
      assert.equal(ir.includes('"kind":"persistence"'),false);
      for(const file of original)assert.equal(hash(fs.readFileSync(join(root,file.path))),file.digest);
      results.push({lane:'adoption-scan-model',scopedPreviewReadOnly:true,manifestedAdapter:true,confirmedBindings:confirmed,strictAuthorization:true,noHttpOrDatabase:true,agentPrompt:'installed-and-current',contextDigest:hash(bytes(context)),irDigest:authorization.model_ref.ir_digest});
      save(join(home,'semantic-acceptance.json'),results[0]);
    }
  });
  const expected=process.platform==='win32'?0:4;
  const report=verify(f,expected);
  if(expected===0){assert.equal(report.status,'passed');assert.equal(report.native.run.commands.length,4);assert.ok(report.native.run.commands.every(row=>row.exit?.value===0));}
  else assert.equal(report.status,'unsupported');
  for(const file of original)assert.equal(hash(fs.readFileSync(join(f.root,file.path))),file.digest);
  results.push({lane:'actual-native-verify',status:report.status,executionQualified:expected===0,planDigest:f.plan.plan_digest,reportDigest:hash(bytes(report)),originalSourcePreserved:true});
  const summary={ok:true,gate:'mvp-user-workflow',binaryDigest:hash(fs.readFileSync(binary)),runtime:process.versions.node,tools:catalog.tools.map(({id,version})=>({id,version})),results};
  if(process.env.LEKALO_NATIVE_REPORTS_DIR)save(join(process.env.LEKALO_NATIVE_REPORTS_DIR,'workflow.json'),summary);
  console.log(JSON.stringify(summary));
} finally {cleanup();}
