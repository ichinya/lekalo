const fs=require('fs'),path=require('path');
const base=process.env.USERPROFILE+'/.pi/agent/sessions';
for(const w of ['45','70']){
  const d=path.join(base,'--C--Users-User-orca-workspaces-lekalo-m4-issue-'+w+'--');
  const files=fs.readdirSync(d).filter(f=>f.endsWith('.jsonl')).sort().reverse();
  const f=path.join(d,files[0]);
  const lines=fs.readFileSync(f,'utf8').split('\n').filter(Boolean);
  console.log('==== issue-'+w);
  for(const l of lines){
    try{const j=JSON.parse(l);const s=JSON.stringify(j);
      if(/provider|model|error|abort|stopReason|finishReason/i.test(s)){
        const m=j.message||{};
        console.log(' ',j.type,'|',(m.provider||''),(m.model||''),(m.api||''),'|',s.slice(0,260).replace(/\s+/g,' '));
      }
    }catch(e){}
  }
}
