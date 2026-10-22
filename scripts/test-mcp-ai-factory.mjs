#!/usr/bin/env node
// Install the committed extension through the real, pinned AI Factory CLI,
// then use its rendered MCP config with the official client and real Lekalo.
// No model inference, application execution, or user configuration writes.
import './registry-union.mjs';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {copyFileSync,cpSync,existsSync,mkdtempSync,mkdirSync,readFileSync,realpathSync,rmSync,writeFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {tmpdir} from 'node:os';
import {basename,dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const factory=process.env.LEKALO_AI_FACTORY_DIR;
const executable=process.platform==='win32'?'lekalo.exe':'lekalo';
const binary=process.env.LEKALO_MCP_BIN??join(root,'target/debug',executable);
if(!factory||!existsSync(binary)){
 process.stderr.write('mcp-ai-factory unavailable: provision ai-factory@2.19.0, set LEKALO_AI_FACTORY_DIR, and build Lekalo\n');
 process.exit(2);
}
const json=path=>JSON.parse(readFileSync(path,'utf8'));
const factoryPackage=json(join(factory,'package.json'));
assert.equal(factoryPackage.name,'ai-factory');
assert.equal(factoryPackage.version,'2.19.0','qualified CLI version');
const require=createRequire(import.meta.url);
const {Client}=require('@modelcontextprotocol/sdk/client/index.js');
const {StdioClientTransport}=require('@modelcontextprotocol/sdk/client/stdio.js');
const sdkVersion=json(join(dirname(require.resolve('@modelcontextprotocol/sdk/client/index.js')),'../../../package.json')).version;
assert.equal(sdkVersion,'1.32.1','qualified MCP client version');
const validatorRequire=createRequire(join(process.env.LEKALO_AJV_NODE_PATH??root,'package.json'));
assert.equal(validatorRequire('ajv/package.json').version,'8.20.0');
const Ajv=validatorRequire('ajv/dist/2020').default;
const validate=new Ajv({strict:true,allErrors:true}).compile(json(join(factory,'schemas/extension.schema.json')));
const manifestPath=join(root,'examples/mcp/ai-factory-extension.json');
const manifest=json(manifestPath);
assert.ok(validate(manifest),JSON.stringify(validate.errors));
const wrongShape={...manifest,mcpServers:{'lekalo-mcp':manifest.mcpServers[0].template}};
assert.equal(validate(wrongShape),false,'object-shaped mcpServers must refuse');
const wrongCwd=structuredClone(manifest);
wrongCwd.mcpServers[0].template.cwd='${workspaceRoot}';
assert.equal(validate(wrongCwd),false,'unsupported cwd must refuse');

const tempRoot=realpathSync(tmpdir());
const scratch=mkdtempSync(join(tempRoot,'lekalo-aif33-'));
let client;
try{
 const project=join(scratch,'project'),extension=join(scratch,'extension'),bin=join(scratch,'bin');
 cpSync(join(root,'tests/fixtures/inspect/planner'),project,{recursive:true});
 mkdirSync(extension);mkdirSync(bin);
 copyFileSync(manifestPath,join(extension,'extension.json'));
 copyFileSync(binary,join(bin,executable));
 writeFileSync(join(project,'.ai-factory.json'),JSON.stringify({version:factoryPackage.version,agents:[{id:'claude',skillsDir:'.claude/skills',installedSkills:[],mcp:{github:false,filesystem:false,postgres:false,chromeDevtools:false,playwright:false}}],extensions:[]}));
 const env={...process.env};
 const pathKey=Object.keys(env).find(key=>key.toLowerCase()==='path')??'PATH';
 const searchPath=bin+(process.platform==='win32'?';':':')+(env[pathKey]??'');
 // The SDK merges a default PATH into its child environment. Keep one
 // spelling so Windows cannot prefer the inherited path over our bin.
 for(const key of Object.keys(env))if(key.toLowerCase()==='path')delete env[key];
 env.PATH=searchPath;
 const installation=spawnSync(process.execPath,[join(factory,'bin/ai-factory.js'),'extension','add',extension],{cwd:project,env,encoding:'utf8',timeout:60000,maxBuffer:4*1024*1024});
 assert.ifError(installation.error);
 assert.equal(installation.status,0,installation.stdout+installation.stderr);
 const installed=json(join(project,'.ai-factory.json'));
 assert.ok(installed.extensions.some(item=>item.name===manifest.name&&item.version===manifest.version));
 const rendered=json(join(project,'.mcp.json')).mcpServers['lekalo-mcp'];
 assert.deepEqual(rendered,manifest.mcpServers[0].template,'actual CLI-rendered configuration');
 const launchEnv={...env,...rendered.env};
 const init={jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'lekalo-ai-factory-qualification',version:'0.8.0'}}};
 const probe=spawnSync(rendered.command,rendered.args,{cwd:project,env:launchEnv,input:JSON.stringify(init)+'\n',encoding:'utf8',timeout:30000,maxBuffer:2*1024*1024});
 assert.ifError(probe.error);assert.equal(probe.status,0,probe.stderr);
 const initialized=probe.stdout.split('\n').filter(Boolean).map(line=>JSON.parse(line)).find(item=>item.id===1);
 assert.equal(initialized.result.protocolVersion,'2025-11-25');
 assert.deepEqual(initialized.result.capabilities.resources,{subscribe:true,listChanged:false});
 const transport=new StdioClientTransport({command:rendered.command,args:rendered.args,cwd:project,env:launchEnv,stderr:'pipe'});
 let serverStderr='';
 transport.stderr?.on('data',chunk=>{serverStderr+=chunk;});
 client=new Client({name:'lekalo-ai-factory-qualification',version:'0.8.0'},{capabilities:{}});
 try{await client.connect(transport);}catch(error){throw new Error(`installed MCP connection failed: ${error.message}; ${serverStderr}`,{cause:error});}
 assert.equal(client.getServerVersion().name,'lekalo');
 const tools=await client.listTools();
 assert.deepEqual(tools.tools.map(tool=>tool.name).sort(),['lekalo_context','lekalo_graph','lekalo_impact','lekalo_inspect','lekalo_target_status','lekalo_trace_export','lekalo_validate']);
 assert.ok(tools.tools.every(tool=>tool.annotations?.readOnlyHint===true));
 const context=await client.callTool({name:'lekalo_context',arguments:{projectId:'project',mode:'symbol',symbol:'planner.focus_task',budget:5000}});
 assert.ok(!context.isError);assert.equal(context.structuredContent.status,'valid');
 assert.equal(context.structuredContent.result.context.budget.limit,5000);
 const resource=await client.readResource({uri:'lekalo://project/summary'});
 assert.equal(resource.contents[0].mimeType,'application/json');
 assert.equal(JSON.parse(resource.contents[0].text).schemaVersion,'lekalo/mcp-resource/v0.8.0');
 console.log(JSON.stringify({ok:true,gate:'mcp-ai-factory',factory:{name:factoryPackage.name,version:factoryPackage.version},sdkVersion,protocol:initialized.result.protocolVersion,manifestSha256:createHash('sha256').update(readFileSync(manifestPath)).digest('hex'),checks:['upstream manifest schema','invalid manifest controls','real extension add','rendered configuration','initialize','tools/list','one-call context','resources/read'],tools:tools.tools.length,modelInference:false,userConfigurationChanged:false}));
}finally{
 await client?.close().catch(()=>{});
 assert.equal(dirname(resolve(scratch)),tempRoot);
 assert.ok(basename(scratch).startsWith('lekalo-aif33-'));
 rmSync(scratch,{recursive:true,force:true});
}
