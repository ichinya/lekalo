const fs=require('fs'),path=require('path');
const d=path.join(process.env.USERPROFILE,'.pi/agent/sessions/--C--Users-User-orca-workspaces-lekalo-m4-issue-70--');
const f=fs.readdirSync(d).filter(x=>x.endsWith('.jsonl')).sort().reverse()[0];
const lines=fs.readFileSync(path.join(d,f),'utf8').split('\n').filter(Boolean);
for(const l of lines.slice(-8)) console.log(l.slice(0,500),'\n---');
