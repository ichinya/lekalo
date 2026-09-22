const fs=require('fs'),path=require('path');
const base=process.env.USERPROFILE+'/.pi/agent/sessions';
for(const w of ['70','85','32','69','117','45']){
  const d=path.join(base,'--C--Users-User-orca-workspaces-lekalo-m4-issue-'+w+'--');
  let files=[];
  try{files=fs.readdirSync(d).filter(f=>f.endsWith('.jsonl')).sort().reverse()}catch(e){console.log(w,'NO DIR');continue}
  if(!files.length){console.log(w,'EMPTY');continue}
  const f=path.join(d,files[0]);
  const lines=fs.readFileSync(f,'utf8').split('\n').filter(Boolean);
  let userMsgs=0,assistantMsgs=0,last='';
  for(const l of lines){try{const j=JSON.parse(l);const t=j.type||j.role||'';if(/user|prompt/i.test(t))userMsgs++;if(/assistant|model/i.test(t))assistantMsgs++;last=JSON.stringify(j).slice(0,180)}catch(e){}}
  console.log('issue-'+w,'|',files[0],'| entries:',lines.length,'| user:',userMsgs,'| assistant:',assistantMsgs);
  console.log('   last:',last);
}
