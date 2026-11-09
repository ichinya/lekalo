/** Native declaration evidence for the explicitly negotiated 1.1.0 scan. */
const keys = new Set(["native","qualified_name","family","path","line","end_line","slot","declaration_only","member_of","signature"]);
const families = new Set(["class","interface","enum","type-alias","function","method","variable","property","module","symbol"]);
const text = (s, max) => typeof s === "string" && [...s].length > 0 && [...s].length <= max && !/[\u0000-\u001f\u007f-\u009f]/u.test(s);
const digest = s => typeof s === "string" && /^sha256:[0-9a-f]{64}$/.test(s);
const compare = (a,b) => Buffer.compare(Buffer.from(a),Buffer.from(b));
/** Validate a complete ordered owner forest without dropping malformed rows. */
export function validNativeSymbols(rows, isLogicalPath) {
  if (!Array.isArray(rows) || rows.length > 10000) return false;
  const all = new Map();
  let previous = "";
  for (const row of rows) {
    if (!row || typeof row !== "object" || Array.isArray(row)
      || Object.keys(row).some(k => !keys.has(k))
      || !text(row.native,256) || compare(row.native,previous) <= 0
      || !text(row.qualified_name,256) || typeof row.family !== "string"
      || !isLogicalPath(row.path)
      || !Number.isSafeInteger(row.line) || row.line < 1
      || !Number.isSafeInteger(row.end_line) || row.end_line < row.line || row.end_line > 1000000
      || !["static","instance"].includes(row.slot) || typeof row.declaration_only !== "boolean"
      || (Object.hasOwn(row,"member_of") && !text(row.member_of,256))
      || (Object.hasOwn(row,"signature") && !digest(row.signature))) return false;
    const parts = row.family.split("+");
    if (parts.length > 10 || parts.some(p=>!families.has(p)) || parts.some((p,i)=>i>0&&parts[i-1]>=p)) return false;
    all.set(row.native,row); previous=row.native;
  }
  const owners = new Map();
  for (const row of rows) {
    const seen = new Set(); let current = row;
    while (current.member_of !== undefined) {
      if (seen.size >= 64 || seen.has(current.native)) return false;
      seen.add(current.native);
      const owner = all.get(current.member_of);
      if (!owner || owner.path !== current.path || owner.line > current.line || owner.end_line < current.end_line) return false;
      current=owner;
    }
    const count=(owners.get(current.native)??0)+1;owners.set(current.native,count);
    if(count>256)return false;
  }
  return true;
}
/** Capture the exact scanner declaration rows, excluding comments/raw source. */
export function nativeInventory(index) {
  return index.symbols.map(s=>({
    native:s.native,qualified_name:s.qualifiedName,family:s.family,path:s.module,
    line:s.line,end_line:s.endLine,slot:s.slot,declaration_only:s.declarationOnly,
    ...(s.memberOf===null?{}:{member_of:s.memberOf}),
    ...(s.signature===null?{}:{signature:s.signature}),
  })).sort((a,b)=>compare(a.native,b.native));
}
/** One bounded complete root subtree, not a depth-truncated projection. */
export function ownedNativeSymbols(rows,native) {
  const byKey = new Map(rows.map(s=>[s.native,s]));
  return rows.filter(s=>{
    let current=s;
    while(current){if(current.native===native)return true;current=byKey.get(current.member_of);}
    return false;
  });
}
