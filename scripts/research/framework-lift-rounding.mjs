#!/usr/bin/env node
// #167 supported-subset workload and public adoption gate. No model calls.
// LEKALO_BIN=... LEKALO_TSC=... node scripts/research/framework-lift-rounding.mjs
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {workspaceToolSource} from './framework-lift-workspace.mjs';
export const repo=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
export const sha=b=>createHash('sha256').update(b).digest('hex');
export function write(root,name,value){const p=join(root,name);fs.mkdirSync(dirname(p),{recursive:true});fs.writeFileSync(p,typeof value==='string'?value:JSON.stringify(value,null,2)+'\n');}
export function inventory(root,prefix=''){return fs.readdirSync(join(root,prefix),{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).flatMap(e=>{const p=prefix?prefix+'/'+e.name:e.name;return e.isDirectory()?inventory(root,p):[{path:p,sha256:sha(fs.readFileSync(join(root,p))),bytes:fs.statSync(join(root,p)).size}];});}
export function exec(program,args,cwd,expected=0){const r=spawnSync(program,args,{cwd,encoding:'utf8',timeout:120000,maxBuffer:16*1024*1024,windowsHide:true});assert.equal(r.status,expected,program+' '+args.slice(0,5).join(' ')+'\n'+r.stdout+'\n'+r.stderr+'\n'+(r.error??''));return r;}
export const task=[
'Extend this dependency-free TypeScript pricing library with opt-in half-even rounding of discounted cents.',
"The exported rounding union must accept 'floor' | 'half_even'. Omitted/undefined mode keeps legacy floor behavior.",
'discount(amount, bps, mode) computes amount * (10000-bps) / 10000 in cents.',
'For half_even, round to the nearest integer; an exact .5 tie goes to the even integer. Round the final discounted amount, not the discount portion.',
'Propagate the optional mode through net(unit, quantity, bps, mode), quote(unit, quantity, bps, shipping, mode), invoice(unit, quantity, bps, tax_bps, mode), and preview(unit, quantity, bps, mode).',
'quote adds shipping AFTER discount; invoice adds the existing floor-rounded tax AFTER discount; preview retains its existing string format.',
"discount, net, quote, invoice and preview must reject all other runtime mode values, including null, numbers, empty and unknown strings, with RangeError('INVALID_ROUNDING').",
'Preserve existing input validation, numeric bounds, exports, and unrelated shipping/tax/fee/credit/loyalty behavior. Do not mutate anything.',
'Inputs are integer cents 0..100000000, quantity 0..100, bps/tax_bps 0..10000; existing subtotal validation rejects totals above 100000000.',
'Only modify src/ (and optional lekalo/ metadata if it exists). Do not modify tests, tools, package files, README or configuration.',
'For source inspection use node tools/workspace.mjs list, search TEXT, or read PATH [PATH...] so delivered-content provenance is recorded. Editing tools may be used normally.',
'Run node --test test/public.test.mjs before submitting. No commits, external services, subagents, or access outside the workspace. Finish with a brief summary.',
''].join('\n');
const native={
'package.json':JSON.stringify({name:'pricing',version:'1.0.0',private:true,type:'module',scripts:{test:'node --test test/public.test.mjs'}},null,2)+'\n',
'tsconfig.json':JSON.stringify({compilerOptions:{target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',strict:true,noEmit:true,allowImportingTsExtensions:true,types:[]},include:['src/**/*.ts']},null,2)+'\n',
'README.md':'# Synthetic pricing library\nPure TypeScript functions, Node 24, no installed packages or backend. Source under src/. Run node --test test/public.test.mjs. Native source inspection: node tools/workspace.mjs list | search TEXT | read PATH [PATH...].\n',
'src/rounding.ts':"export type rounding = 'floor';\n",
'src/checks.ts':"/** Validate one integer without changing its value. */\nexport function bounded(value: number, maximum: number): number {\n  if (typeof value !== 'number' || value < 0 || value > maximum || Math.floor(value) !== value) throw new RangeError('INVALID_NUMBER');\n  return value;\n}\n",
'src/subtotal.ts':"import {bounded} from './checks.ts';\n/** Validated undiscounted item total, in cents. */\nexport function subtotal(unit: number, quantity: number): number {\n  return bounded(bounded(unit, 100000000) * bounded(quantity, 100), 100000000);\n}\n",
'src/discount.ts':"import {bounded} from './checks.ts';\nimport type {rounding} from './rounding.ts';\n/** Discounted amount in cents. This legacy version supports floor only. */\nexport function discount(amount: number, bps: number, mode: rounding = 'floor'): number {\n  if (mode !== 'floor') throw new RangeError('INVALID_ROUNDING');\n  return Math.floor(bounded(amount, 100000000) * (10000 - bounded(bps, 10000)) / 10000);\n}\n",
'src/net.ts':"import {subtotal} from './subtotal.ts';\nimport {discount} from './discount.ts';\n/** Discounted item total before shipping and tax. */\nexport function net(unit: number, quantity: number, bps: number): number {\n  return discount(subtotal(unit, quantity), bps);\n}\n",
'src/quote.ts':"import {net} from './net.ts';\nimport {bounded} from './checks.ts';\n/** Shipping is added after discount and is not itself discounted. */\nexport function quote(unit: number, quantity: number, bps: number, shipping: number): number {\n  return net(unit, quantity, bps) + bounded(shipping, 100000000);\n}\n",
'src/tax.ts':"import {bounded} from './checks.ts';\n/** Existing tax always rounds down, independently of discount policy. */\nexport function tax(amount: number, bps: number): number {\n  return Math.floor(bounded(amount, 100000000) * bounded(bps, 10000) / 10000);\n}\n",
'src/invoice.ts':"import {net} from './net.ts';\nimport {tax} from './tax.ts';\n/** Existing tax is calculated on discounted cents, then added. */\nexport function invoice(unit: number, quantity: number, bps: number, tax_bps: number): number {\n  return net(unit, quantity, bps) + tax(net(unit, quantity, bps), tax_bps);\n}\n",
'src/preview.ts':"import {net} from './net.ts';\n/** Stable client text boundary: integer cents plus the literal suffix. */\nexport function preview(unit: number, quantity: number, bps: number): string {\n  return net(unit, quantity, bps) + ' cents';\n}\n",
'src/shipping.ts':"import {bounded} from './checks.ts';\n/** Independent weight bracket: do not apply discount rounding here. */\nexport function shipping(grams: number): number {\n  return Math.floor(bounded(grams, 100000000) / 500) * 25 + 100;\n}\n",
'src/fee.ts':"import {bounded} from './checks.ts';\n/** Existing processing fee is floor-based and capped. */\nexport function fee(amount: number): number {\n  return Math.min(500, Math.floor(bounded(amount, 100000000) * 15 / 10000));\n}\n",
'src/credit.ts':"import {bounded} from './checks.ts';\n/** Credit cannot make the balance negative. */\nexport function credit(amount: number, balance: number): number {\n  return Math.max(0, bounded(amount, 100000000) - bounded(balance, 100000000));\n}\n",
'src/loyalty.ts':"import {bounded} from './checks.ts';\n/** Independent loyalty points remain whole floor-rounded units. */\nexport function loyalty(amount: number): number {\n  return Math.floor(bounded(amount, 100000000) / 1000);\n}\n",
'src/index.ts':['rounding','checks','subtotal','discount','net','quote','tax','invoice','preview','shipping','fee','credit','loyalty'].map(n=>"export * from './"+n+".ts';").join('\n')+'\n',
'test/public.test.mjs':"import {test} from 'node:test';\nimport assert from 'node:assert/strict';\nimport {discount,net,quote,invoice,preview,subtotal,tax,shipping,fee,credit,loyalty} from '../src/index.ts';\ntest('legacy calculation and rounding',()=>{assert.equal(discount(5,5000),2);assert.equal(net(5,1,5000),2);assert.equal(quote(5,1,5000,10),12);assert.equal(invoice(5,1,5000,5000),3);assert.equal(preview(5,1,5000),'2 cents');});\ntest('input boundaries',()=>{assert.throws(()=>subtotal(-1,1),RangeError);assert.throws(()=>discount(3,10001),RangeError);assert.throws(()=>subtotal(100000000,2),RangeError);});\ntest('independent policies',()=>{assert.equal(tax(3,5000),1);assert.equal(shipping(751),125);assert.equal(fee(1000),1);assert.equal(credit(20,30),0);assert.equal(loyalty(1999),1);});\n",
'.gitignore':'.lekalo/cache/\n.benchmark/\n'
};
native['tools/workspace.mjs']=workspaceToolSource();

export const oracleSource=[
"import assert from 'node:assert/strict';import {pathToFileURL} from 'node:url';import {resolve} from 'node:path';",
"const m=await import(pathToFileURL(resolve(process.argv[2],'src/index.ts')).href);const rows=[];",
"const check=(name,fn)=>{try{fn();rows.push({name,passed:true});}catch(e){rows.push({name,passed:false,error:String(e.message).slice(0,260)});}};",
"const equal=(a,b)=>assert.equal(a,b);const invalid=fn=>assert.throws(fn,e=>e instanceof RangeError&&e.message==='INVALID_ROUNDING');",
"for(const [a,b,w,n]of [[5,5000,2,'tie-even-down'],[7,5000,4,'tie-odd-up'],[1,5000,0,'tie-zero'],[3,5000,2,'tie-two'],[10,3333,7,'above-half'],[10,6667,3,'below-half'],[9,0,9,'zero-discount'],[9,10000,0,'full-discount'],[100000000,1,99990000,'maximum'],[99999999,5000,50000000,'large-tie']])check(n,()=>equal(m.discount(a,b,'half_even'),w));",
"check('round-final-not-saved-portion',()=>equal(m.discount(5,3000,'half_even'),4));",
"check('net-propagates',()=>equal(m.net(7,1,5000,'half_even'),4));",
"check('quote-shipping-after',()=>equal(m.quote(7,1,5000,3,'half_even'),7));",
"check('invoice-tax-after',()=>equal(m.invoice(7,1,5000,5000,'half_even'),6));",
"check('preview-propagates',()=>equal(m.preview(7,1,5000,'half_even'),'4 cents'));",
"check('legacy-floor',()=>{equal(m.discount(7,5000),3);equal(m.discount(7,5000,'floor'),3);equal(m.discount(7,5000,undefined),3);equal(m.net(7,1,5000),3);equal(m.quote(7,1,5000,3),6);equal(m.invoice(7,1,5000,5000),4);equal(m.preview(7,1,5000),'3 cents');});",
"check('invalid-modes-every-entry',()=>{for(const mode of [null,0,1,'','HALF_EVEN','normal',{},[]])for(const fn of [()=>m.discount(7,5000,mode),()=>m.net(7,1,5000,mode),()=>m.quote(7,1,5000,3,mode),()=>m.invoice(7,1,5000,5000,mode),()=>m.preview(7,1,5000,mode)])invalid(fn);});",
"check('existing-validation',()=>{for(const fn of [()=>m.discount(-1,0,'half_even'),()=>m.discount(1,10001,'half_even'),()=>m.net(100000000,2,0,'half_even'),()=>m.discount(NaN,0,'half_even'),()=>m.discount(1.5,0,'half_even')])assert.throws(fn,e=>e instanceof RangeError&&e.message==='INVALID_NUMBER');});",
"check('unrelated-rounding',()=>{equal(m.tax(3,5000),1);equal(m.fee(1000),1);equal(m.shipping(751),125);equal(m.loyalty(1999),1);equal(m.credit(20,30),0);});",
"check('independent-rational-oracle',()=>{for(let a=0;a<120;a++)for(const b of [0,1,1250,2500,3333,5000,6667,7500,9999,10000]){const n=BigInt(a)*BigInt(10000-b),q=n/10000n,r=n%10000n;const w=Number(q+(r>5000n||(r===5000n&&q%2n!==0n)?1n:0n));equal(m.discount(a,b,'half_even'),w);}});",
"const passed=rows.filter(x=>x.passed).length;console.log(JSON.stringify({passed,total:rows.length,rows},null,2));process.exitCode=passed===rows.length?0:1;",""].join('\n');
export function reference(root){
  write(root,'src/rounding.ts',"export type rounding = 'floor' | 'half_even';\n");
  write(root,'src/discount.ts',"import {bounded} from './checks.ts';\nimport type {rounding} from './rounding.ts';\nexport function discount(amount:number,bps:number,mode:rounding='floor'):number {\nif(mode!=='floor'&&mode!=='half_even')throw new RangeError('INVALID_ROUNDING');\nconst numerator=bounded(amount,100000000)*(10000-bounded(bps,10000));const floor=Math.floor(numerator/10000);const rest=numerator%10000;\nreturn mode==='floor'?floor:floor+(rest>5000||(rest===5000&&floor%2!==0)?1:0);\n}\n");
  for(const name of ['net','quote','invoice','preview']){
    let s=fs.readFileSync(join(root,'src',name+'.ts'),'utf8');
    const anchor=name==='quote'?'shipping: number)':name==='invoice'?'tax_bps: number)':'bps: number)';
    s="import type {rounding} from './rounding.ts';\n"+s.replace(anchor,anchor.slice(0,-1)+", mode: rounding = 'floor')").replaceAll('quantity, bps)','quantity, bps, mode)').replace('quantity), bps)','quantity), bps, mode)');
    write(root,'src/'+name+'.ts',s);
  }
}
export function semantic(root){
  const types=[{id:'pricing.cents',kind:'scalar',version:1,base:'number',description:'Integer cents 0..100000000; native checks enforce the range.'},{id:'pricing.quantity',kind:'scalar',version:1,base:'number',description:'Integer item count 0..100.'},{id:'pricing.bps',kind:'scalar',version:1,base:'number',description:'Integer basis points 0..10000.'},{id:'pricing.text',kind:'scalar',version:1,base:'string'},{id:'pricing.rounding',kind:'enum',version:1,description:'Existing baseline discount policy. Other runtime values must be rejected.',values:[{value:'floor'}]}];
  const field=(name,type,required=true)=>({name,type:'pricing.'+type,...(required?{required:true}:{default:'floor'})});
  const inputs={bounded:[field('value','cents'),field('maximum','cents')],subtotal:[field('unit','cents'),field('quantity','quantity')],discount:[field('amount','cents'),field('bps','bps'),field('mode','rounding',false)],net:[field('unit','cents'),field('quantity','quantity'),field('bps','bps')],quote:[field('unit','cents'),field('quantity','quantity'),field('bps','bps'),field('shipping','cents')],tax:[field('amount','cents'),field('bps','bps')],invoice:[field('unit','cents'),field('quantity','quantity'),field('bps','bps'),field('tax_bps','bps')],preview:[field('unit','cents'),field('quantity','quantity'),field('bps','bps')],shipping:[field('grams','cents')],fee:[field('amount','cents')],credit:[field('amount','cents'),field('balance','cents')],loyalty:[field('amount','cents')]};
  const descriptions={bounded:'Reject non-integer, negative or over-maximum numbers with INVALID_NUMBER; return the original value.',subtotal:'Multiply validated unit cents and quantity. Reject a total above 100000000.',discount:'Return floor(amount * (10000-bps) / 10000). Mode defaults to floor; reject other modes with INVALID_ROUNDING. Pure calculation, no state writes.',net:'Call subtotal then discount; no shipping or tax. Returns discounted item cents.',quote:'Call net then add validated shipping; never discount shipping.',tax:'Floor-rounded tax amount. Its rounding is independent of discount policy and must stay unchanged.',invoice:'Call net, calculate tax on that discounted amount, add that tax. Discount policy must not alter tax rounding.',preview:"Call net and append exactly ' cents'.",shipping:'Independent floor-based 500g shipping brackets plus 100 cents.',fee:'Independent floor-based fee, capped at 500 cents.',credit:'Subtract validated balance, clamp to zero.',loyalty:'Independent floor-based points per 1000 cents.'};
  const commands=Object.entries(inputs).map(([name,input])=>({id:'pricing.'+name,kind:'command',version:1,description:descriptions[name],input,returns:'pricing.'+(name==='preview'?'text':'cents')}));
  write(root,'lekalo/modules/pricing/entities.yaml',{schema_version:'0.8.0',definitions:types});
  write(root,'lekalo/modules/pricing/commands.yaml',{schema_version:'0.8.0',definitions:commands});
  write(root,'lekalo/modules/pricing/scenarios.yaml',{schema_version:'0.8.0',definitions:[{id:'pricing.discount_flow',kind:'scenario',version:1,summary:'Discount policy flows through net, quote, invoice and preview; shipping and tax are applied after discount.',covers:['pricing.discount','pricing.net','pricing.quote','pricing.invoice','pricing.preview']},{id:'pricing.independent_policies',kind:'scenario',version:1,summary:'Tax, shipping, fees, credits and loyalty keep their independent existing policies.',covers:['pricing.tax','pricing.shipping','pricing.fee','pricing.credit','pricing.loyalty']}]});
}
export function prepare(options={}){
  const binary=options.binary??process.env.LEKALO_BIN??join(repo,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo'),tsc=options.tsc??process.env.LEKALO_TSC;
  assert.ok(tsc&&fs.existsSync(tsc),'LEKALO_TSC must name the pinned TypeScript compiler');assert.ok(fs.existsSync(binary),'LEKALO_BIN must name the CLI');
  const root=fs.mkdtempSync(join(tmpdir(),'lekalo-167-rounding-')),baseline=join(root,'baseline'),adopted=join(root,'adopted'),gold=join(root,'reference');
  for(const [name,value]of Object.entries(native))write(baseline,name,value);
  write(root,'task.txt',task);write(root,'oracle.mjs',oracleSource);
  exec(process.execPath,['--test','test/public.test.mjs'],baseline);exec(process.execPath,[tsc,'-p','tsconfig.json'],baseline);
  const before=inventory(baseline);fs.cpSync(baseline,adopted,{recursive:true});const receipts=[];
  function cli(args,{json=true}={}){const r=exec(binary,[...(json?['--json']:[]),'--no-cache',...args],adopted);const doc=json?JSON.parse(r.stdout):r.stdout;receipts.push({args,exit:r.status,stdout:r.stdout,stderr:r.stderr});return doc;}
  const started=performance.now();
  cli(['init','--adopt','--project-id','rounding_campaign','--project','.','--dry-run']);cli(['init','--adopt','--project-id','rounding_campaign','--project','.']);cli(['module','new','pricing','--project','.']);
  // Explicitly authored successor fixture; discovery does not invent semantics.
  for(const p of ['lekalo/project.yaml','lekalo/modules/pricing/module.yaml'])write(adopted,p,fs.readFileSync(join(adopted,p),'utf8').replaceAll('0.2.16','0.8.0'));
  semantic(adopted);cli(['validate','--strict','--project','.']);
  const profile={id:'standalone',mode:'observed',target:'node-typescript',readRoots:[{kind:'file',path:'package.json'},{kind:'file',path:'tsconfig.json'},{kind:'tree',path:'src'}],exclusions:[],provenance:{origin:'declared',revision:'rounding-baseline-v1',disposition:'public-fixture'}};
  write(root,'scan-profile.json',profile);
  const scan=cli(['scan','--target','node-typescript','--profile','standalone','--project','.',process.execPath,join(repo,'adapters/node-typescript/adapter.mjs'),'--lekalo-project-profile-json',JSON.stringify(profile)]);
  const proposals=cli(['bindings','propose','--project','.']);assert.equal(proposals.ambiguous,0);
  for(const p of proposals.proposals){assert.equal(p.candidates.length,1);assert.ok(fs.existsSync(join(adopted,p.candidates[0].path)));cli(['bindings','confirm',p.proposal,'--project','.']);}
  const audit=cli(['bindings','audit','--project','.']);assert.equal(audit.current,scan.symbols);assert.equal(audit.unknown,0);
  cli(['validate','--strict','--project','.']);const ir=cli(['load','--ir','--project','.']);const adoptionMs=Math.round(performance.now()-started);
  const ca=['context','pricing.discount','--budget','1600','--project','.'],ia=['impact','pricing.discount','--project','.'];
  const context=cli(ca,{json:false}),impact=cli(ia,{json:false}),contextJson=cli(ca),impactJson=cli(ia);
  const inspected=cli(['observe','inspect','pricing.discount','--project','.']);assert.equal(inspected.binding,'confirmed');assert.equal(inspected.state,'current');assert.equal(inspected.location.path,'src/discount.ts');
  for(const f of before)assert.equal(sha(fs.readFileSync(join(adopted,f.path))),f.sha256,'adoption changed native file '+f.path);
  fs.cpSync(baseline,gold,{recursive:true});reference(gold);exec(process.execPath,[tsc,'-p','tsconfig.json'],gold);exec(process.execPath,['--test','test/public.test.mjs'],gold);
  const grade=(dir,exit)=>JSON.parse(exec(process.execPath,[join(root,'oracle.mjs'),dir],root,exit).stdout);
  const referenceGrade=grade(gold,0),baselineGrade=grade(baseline,1);assert.equal(referenceGrade.passed,20);assert.ok(baselineGrade.passed<20);
  write(root,'adoption-receipts.private.json',receipts);write(root,'oracle-preflight.json',{reference:referenceGrade,baseline:baselineGrade});write(root,'context.md',context);write(root,'impact.txt',impact);write(root,'context.json',contextJson);write(root,'impact.json',impactJson);
  const result={root,binary,tsc,sourceCommit:exec('git',['rev-parse','HEAD'],repo).stdout.trim(),builderDigest:sha(fs.readFileSync(fileURLToPath(import.meta.url))),nativeFiles:before,nativeDigest:sha(JSON.stringify(before)),modelDigest:sha(JSON.stringify(inventory(join(adopted,'lekalo')))),irDigest:sha(JSON.stringify(ir.ir)),adoption:{sourcePreserved:true,publicScan:true,symbols:scan.symbols,confirmed:audit.current,adoptionMs,authoringHumanTime:'unknown; fixture authoring and review excluded from CLI duration'},oracle:{reference:referenceGrade.passed+'/'+referenceGrade.total,baseline:baselineGrade.passed+'/'+baselineGrade.total},rendering:{contextMarkdownBytes:Buffer.byteLength(context),impactHumanBytes:Buffer.byteLength(impact),contextJsonBytes:Buffer.byteLength(JSON.stringify(contextJson)),impactJsonBytes:Buffer.byteLength(JSON.stringify(impactJson))},limitations:['Different task from earlier planner pilot; no cross-task percentage comparison.','Supported top-level TypeScript subset only; original nested planner refusal remains.','Authored semantics/scenarios do not prove native conformance.','Public scan uses generic entity carriers for mappings, not inferred command classification.','Cold CLI adoption excludes authoring/review labor and is separate from warm agent runs.']};
  write(root,'preparation.json',result);return result;
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))console.log(JSON.stringify(prepare(),null,2));
