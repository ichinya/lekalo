// Read-only authoring helper; the production host independently rechecks custody.
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
const hash=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const safePath=path=>path.length<=512&&path.split('/').every(part=>part.length>0&&part.length<=128&&part!=='.'&&part!=='..'&&!part.endsWith('.')&&/^[A-Za-z0-9@_.+-]+$/.test(part)&&!['.git','.ssh','.npmrc','.netrc'].includes(part.toLowerCase())&&!part.toLowerCase().startsWith('.env')&&!/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:[.]|$)/i.test(part));
const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
export function createNodeToolCatalog(root) {
  const assets=fs.realpathSync(resolve(root)),files=[],seen=new Set();let directories=0,total=0;
  function walk(prefix='',depth=0) {
    assert.ok(depth<=32,'tool directory depth');
    for(const name of fs.readdirSync(join(assets,prefix))) {
      if(name==='.bin')continue;
      const path=prefix?prefix+'/'+name:name,full=join(assets,path);assert.ok(safePath(path),'unsupported or protected tool path');const meta=fs.lstatSync(full);
      assert.ok(!seen.has(path.toLowerCase()),'case alias');seen.add(path.toLowerCase());
      assert.ok(!meta.isSymbolicLink(),'tool assets must be physical');
      if(meta.isDirectory()){assert.ok(++directories<=4096,'tool directory cap');walk(path,depth+1);}
      else {
        assert.ok(meta.isFile()&&meta.nlink===1,'tool assets must be exclusive regular files');
        assert.ok(files.length<4096&&meta.size<=32*1024*1024,'tool file cap');total+=meta.size;assert.ok(total<=128*1024*1024,'tool byte cap');
        const bytes=fs.readFileSync(full);assert.equal(bytes.length,meta.size,'tool changed during inventory');
        files.push({path,digest:hash(bytes),bytes:bytes.length});
      }
    }
  }
  walk();files.sort((a,b)=>a.path<b.path?-1:a.path>b.path?1:0);
  const digest=hash(JSON.stringify(canonical(files))),program=fs.realpathSync(process.execPath);
  return {schema_version:'lekalo/native-tool-catalog/v1.0.0',runtime:{program,program_digest:hash(fs.readFileSync(program)),version:process.versions.node},assets:{root:assets,manifest_digest:digest,files},tools:[['vitest','vitest/vitest.mjs','vitest'],['tsc','typescript/bin/tsc','typescript'],['vite','vite/bin/vite.js','vite']].map(([id,entry,name])=>({id,entry,entry_digest:hash(fs.readFileSync(join(assets,entry))),artifact_digest:digest,version:JSON.parse(fs.readFileSync(join(assets,name,'package.json'),'utf8')).version}))};
}
