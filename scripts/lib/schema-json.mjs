// Build-time structural schema copying must preserve integer bounds outside
// JavaScript's safe range. Such numbers remain opaque until serialization;
// callers may copy/traverse them but must not perform arithmetic on them.
export function createSchemaJson() {
  const prefix="__lekalo_schema_integer_";
  const integers=new Map(), markers=new Map();
  function parse(text) {
    const encoded=text.replace(/"(?:\\[\s\S]|[^"\\])*"|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/g,token=>{
      if(token.startsWith('"')) {
        if(JSON.parse(token).startsWith(prefix)) throw new Error("reserved schema integer marker");
        return token;
      }
      if(/[.eE]/.test(token)) return token;
      const value=BigInt(token);
      if(value>=BigInt(Number.MIN_SAFE_INTEGER)&&value<=BigInt(Number.MAX_SAFE_INTEGER))return token;
      let marker=integers.get(token);
      if(marker===undefined) {
        marker=`${prefix}${integers.size}__`;
        integers.set(token,marker); markers.set(marker,token);
      }
      return JSON.stringify(marker);
    });
    return JSON.parse(encoded);
  }
  function stringify(value,space) {
    return JSON.stringify(value,null,space).replace(/"__lekalo_schema_integer_\d+__"/g,token=>markers.get(JSON.parse(token))??token);
  }
  return {parse,stringify};
}
