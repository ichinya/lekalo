// Build-time only. No compiler package resolution occurs in deployed adapters.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {readFileSync,readdirSync,lstatSync,existsSync} from 'node:fs';
import {dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {build} from 'esbuild';

const root=dirname(fileURLToPath(import.meta.url));
const require=createRequire(import.meta.url);
const digest=data=>createHash('sha256').update(data).digest('hex');
const MAX_IPC_FRAME=64*1024*1024;

/** Verify every installed API source file against the reviewed npm archive. */
export function verifiedCompilerSources(){
 const pins=JSON.parse(readFileSync(join(root,'compiler-resources.json'),'utf8'));
 assert.equal(pins.format,'lekalo/typescript-native-components/1');
 assert.equal(pins.typescript,'7.0.2');
 const lock=JSON.parse(readFileSync(join(root,'package-lock.json'),'utf8'));
 assert.equal(lock.packages['node_modules/typescript'].integrity,pins.apiPackageIntegrity);
 const packageRoot=dirname(require.resolve('typescript/package.json'));
 assert.equal(JSON.parse(readFileSync(join(packageRoot,'package.json'),'utf8')).version,pins.typescript);
 const rows=[];
 const visit=(directory,prefix='')=>{
  for(const entry of readdirSync(directory,{withFileTypes:true}).sort((a,b)=>a.name<b.name?-1:a.name>b.name?1:0)){
   const path=join(directory,entry.name),name=prefix+entry.name;
   assert.ok(!lstatSync(path).isSymbolicLink(),'compiler API sources cannot contain links');
   if(entry.isDirectory())visit(path,name+'/');
   else{assert.ok(entry.isFile());const data=readFileSync(path);rows.push({path:name,sha256:digest(data),bytes:data.length});}
  }
 };
 visit(packageRoot);
 assert.equal(rows.length,pins.apiSources.files,'compiler API source inventory');
 assert.equal(digest(JSON.stringify(rows)),pins.apiSources.sha256,'compiler API source integrity');
 for(const resource of pins.resources){
  const installed=lock.packages['node_modules/'+resource.package];
  assert.equal(installed?.version,pins.typescript,'native compiler version');
  assert.equal(installed.integrity,resource.integrity,'native compiler archive integrity');
 }
 assert.equal(require('esbuild/package.json').version,'0.28.2');
 const binary=join(root,'node_modules',`@esbuild/${process.platform}-${process.arch}`,process.platform==='win32'?'esbuild.exe':'bin/esbuild');
 assert.ok(existsSync(binary),'provision the exact esbuild platform binary before building');
 return {packageRoot,pins};
}

/** Narrow, source-integrity-checked transport adaptation of the upstream client.
 * The core-owned peer uses inherited fd0/fd1. There is no process-spawn or
 * ambient compiler fallback in the deployed client. Both directions bound
 * MessagePack payloads before allocating or writing them.
 */
function adaptChannel(source){
 const constructor='    constructor(exe, args, collectTiming = false) {';
 assert.equal(source.split(constructor).length,2,'reviewed native channel constructor');
 const start=source.indexOf(constructor),end=source.indexOf('\n    // ─',start);
 assert.ok(end>start,'reviewed native channel constructor boundary');
 source=source.slice(0,start)+constructor+`
        this.collectTiming=collectTiming;
        this.readFd=0;
        this.writeFd=1;
        this.lekaloPeer=true;
        this.child={exitCode:null,kill(){},unref(){}};
    }
`+source.slice(end);
 const spawnImport='import { spawn, } from "node:child_process";';
 assert.equal(source.split(spawnImport).length,2,'reviewed native spawn import');
 source=source.replace(spawnImport,'');
 source=source.replace('Spawns a child process and communicates with it synchronously over','Communicates with a core-owned compiler peer synchronously over');
 source=source.replace('Kill the child process and release resources.','Release the client session; core owns compiler termination.');
 const close='    close() {';
 assert.equal(source.split(close).length,2,'reviewed channel close');
 source=source.replace(close,close+`
        if(this.lekaloPeer){this.closed=true;return;}
`);
 const allocation='const buf = Buffer.allocUnsafeSlow(length);';
 assert.equal(source.split(allocation).length,2,'reviewed incoming frame allocation');
 source=source.replace(allocation,`if(!Number.isSafeInteger(length)||length<0||length>${MAX_IPC_FRAME})throw new Error("compiler IPC frame bound");\n        `+allocation);
 const size='const totalSize = headerSize + payloadLen;';
 assert.equal(source.split(size).length,2,'reviewed outgoing frame');
 source=source.replace(size,size+`\n        if(totalSize>${MAX_IPC_FRAME})throw new Error("compiler IPC frame bound");`);
 return source;
}

/** Return the canonical, pure JS TypeScript 7 API/AST client. */
export async function renderCompilerClient(){
 const {packageRoot,pins}=verifiedCompilerSources();
 const entry=`
export * as ast from ${JSON.stringify(join(packageRoot,'dist/ast/index.js'))};
export * from ${JSON.stringify(join(packageRoot,'dist/api/sync/api.js'))};
export {ModuleResolutionKind} from ${JSON.stringify(join(packageRoot,'dist/enums/moduleResolutionKind.js'))};
export {JsxEmit} from ${JSON.stringify(join(packageRoot,'dist/enums/jsxEmit.js'))};
export {NewLineKind} from ${JSON.stringify(join(packageRoot,'dist/enums/newLineKind.js'))};
export {ModuleDetectionKind} from ${JSON.stringify(join(packageRoot,'dist/enums/moduleDetectionKind.js'))};
function freeze(value){if(value&&typeof value==="object"){for(const v of Object.values(value))freeze(v);Object.freeze(value);}return value;}
export const compilerComponents=freeze(${JSON.stringify(pins)});
`;
 const result=await build({
  absWorkingDir:root,stdin:{contents:entry,resolveDir:root,sourcefile:'compiler-client-entry.mjs'},
  bundle:true,write:false,format:'esm',platform:'node',target:['node18'],charset:'utf8',
  legalComments:'inline',sourcemap:false,logLevel:'silent',metafile:true,
  banner:{js:`// Generated: TypeScript ${pins.typescript} API/AST, Microsoft Corporation, Apache-2.0.\n// See THIRD_PARTY_NOTICES.md. Transport adaptation revision 1; private IPC frame bound ${MAX_IPC_FRAME}.`},
  plugins:[{name:'verified-native-compiler-client',setup(b){
   b.onResolve({filter:/^#getExePath$/},()=>({path:'no-ambient-compiler',namespace:'lekalo-no-ambient'}));
   b.onLoad({filter:/.*/,namespace:'lekalo-no-ambient'},()=>({contents:'export default function(){throw new Error("an explicit verified compiler resource is required");}',loader:'js'}));
   b.onLoad({filter:/[\\/]dist[\\/]api[\\/]syncChannel\.js$/},args=>({contents:adaptChannel(readFileSync(args.path,'utf8')),loader:'js',resolveDir:dirname(args.path)}));
  }}],
 });
 assert.equal(result.outputFiles.length,1);
 for(const output of Object.values(result.metafile.outputs))for(const imported of output.imports){
  assert.notEqual(imported.path,'node:child_process','the deployed compiler client cannot spawn');
  assert.notEqual(imported.path,'node:net','the deployed compiler client cannot open network sockets');
 }
 return Buffer.from(result.outputFiles[0].contents);
}
