const fs=require('fs'),path=require('path');
const base=process.env.USERPROFILE+'/.pi/agent/sessions';
for(const w of ['70','85','32','69','117','45']){
  const d=path.join(base,'--C--Users-User-orca-workspaces-lekalo-m4-issue-'+w+'--');
  let files=[];
  try{files=fs.readdirSync(d).filter(f=>f.endsWith('.jsonl')).sort().reverse()}catch(e){continue}
  if(!files.length){console.log(w,'EMPTY');continue}
  const f=path.join(d,files[0]);
  const st=fs.statSync(f);
  const lines=fs.readFileSync(f,'utf8').split('\n').filter(Boolean);
  const last=lines.slice(-1)[0];
  let desc='';
  try{const j=JSON.parse(last);desc=(j.message?.role||j.type)+' '+(JSON.stringify(j.message?.content||j).slice(0,150))}catch(e){}
  console.log(`issue-${w} | ${lines.length} entries | mtime ${st.mtime.toISOString()} | ${desc}`);
}
