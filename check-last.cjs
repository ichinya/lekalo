const fs=require('fs'),path=require('path');
const base=process.env.USERPROFILE+'/.pi/agent/sessions';
for(const w of ['70','85','32','69','117','45']){
  const d=path.join(base,'--C--Users-User-orca-workspaces-lekalo-m4-issue-'+w+'--');
  let files=[];
  try{files=fs.readdirSync(d).filter(f=>f.endsWith('.jsonl')).sort().reverse()}catch(e){continue}
  if(!files.length)continue;
  const f=path.join(d,files[0]);
  const lines=fs.readFileSync(f,'utf8').split('\n').filter(Boolean);
  console.log('==== issue-'+w+' ('+lines.length+' entries, last '+fs.statSync(f).mtime.toISOString()+')');
  for(const l of lines.slice(-3)){
    try{const j=JSON.parse(l);
      const m=j.message||{};
      const c=(m.content?JSON.stringify(m.content):JSON.stringify(j)).slice(0,300);
      console.log('  ',j.type,m.role||'',c);
    }catch(e){}
  }
}
