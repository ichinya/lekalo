// Closed, independently versioned issue-111 wire families.
const string = {type:'string', maxLength:1024};
const token = {type:'string', minLength:1, maxLength:256, pattern:'^[A-Za-z0-9_.:/@-]+$'};
const path = {type:'string',minLength:1,maxLength:256,pattern:'^(?!.*(?:^|/)(?:\\.\\.?|\\.git|\\.bb)(?:/|$))(?:[A-Za-z0-9_.@-]+/)*[A-Za-z0-9_.@-]+$'};
const digest = {type:'string', pattern:'^sha256:[0-9a-f]{64}$'};
const nullable = value => ({anyOf:[value,{type:'null'}]});
const list = (items, maxItems=512) => ({type:'array', items, maxItems});
const object = properties => ({type:'object',additionalProperties:false,required:Object.keys(properties),properties});
const mode = {enum:['observed','contracted','managed']};
const literal = value => ({const:value});
const header = family => literal(`lekalo/${family}/v1.0.0`);
const observation = object({scenario:token,output:{},error:{},state:{},effects:list({}),authorization:{},transaction:{}});
const runner = object({program:{enum:['node','python','python3','deno','bun']},digest,args:list(string,64),expected:{...list(observation,128),minItems:1}});
const binding = object({symbol:token,path,nativeSymbol:token});
const claim = object({binding,digest,lifecycle:{enum:['generated','scaffolded']}});
const pin = object({path,digest:nullable(digest)});
const request = object({schemaVersion:header('ownership-transition-request'),module:token,symbol:nullable(token),to:mode,
  inputs:list(path),coverage:nullable(path),declaration:nullable(path),unknownBehavior:list(token),runner});
const edit = object({path,before:nullable(digest),after:nullable(digest),beforeContent:nullable({type:'string',maxLength:8388608}),content:nullable({type:'string',maxLength:8388608})});
const semanticChange = object({kind:literal('ownership.mode-changed'),subject:token,from:mode,to:mode});
const plan = object({schemaVersion:header('ownership-transition-plan'),planDigest:digest,project:token,request,from:mode,
  inputs:list(pin),baseline:list(binding),candidate:list(binding),edits:list(edit),claims:list(claim),semanticDiff:list(semanticChange)});
const verification = object({schemaVersion:header('ownership-verification'),planDigest:digest,runnerDigest:digest,
  verdict:{enum:['match','mismatch']},observations:list(object({scenario:token,baselineDigest:nullable(digest),candidateDigest:nullable(digest),expectedDigest:digest}),128),counterexample:nullable(token)});
const receipt = object({schemaVersion:header('ownership-transition-receipt'),planDigest:digest,project:token,
  state:{enum:['committed','replayed','rolled-back']},module:token,symbol:nullable(token),from:mode,to:mode,
  verification,files:list(pin),rollback:digest,previousReceipt:nullable(digest)});
const assignment = object({module:token,symbol:nullable(token),mode,transition:digest,artifacts:list(claim)});
const state = object({schemaVersion:header('ownership-state'),pending:nullable(object({planDigest:digest,journalDigest:digest,
 phase:{enum:['publishing','restoring']},baseDigest:nullable(digest)})),assignments:list(assignment)});
const journal = object({schemaVersion:header('ownership-journal'),phase:{enum:['publishing','restoring']},plan,verification});
const rollback = object({schemaVersion:header('ownership-rollback-plan'),planDigest:digest,phase:token,files:list(path)});
const status = object({schemaVersion:header('ownership-status'),project:token,evidenceBasis:literal('declared-ownership'),
 legacy:list(object({module:token,mode})),assignments:list(assignment)});
const trace = object({schemaVersion:header('ownership-trace'),authority:literal('derived'),project:token,planDigest:digest,
 requestDigest:digest,subject:token,from:mode,to:mode,state:{enum:['committed','replayed','rolled-back']},current:{type:'boolean'},
 semanticDiff:list(semanticChange),inputs:list(pin),verification,files:list(pin),rollback:digest,receiptDigest:digest,previousReceipt:nullable(digest)});
export const ownershipSchemas = Object.fromEntries(Object.entries({
  'ownership-transition-request':request,'ownership-transition-plan':plan,'ownership-verification':verification,
  'ownership-transition-receipt':receipt,'ownership-state':state,'ownership-journal':journal,'ownership-rollback-plan':rollback,
  'ownership-status':status,'ownership-trace':trace,
}).map(([family,body]) => [family,{
  $schema:'https://json-schema.org/draft/2020-12/schema',
  $id:`https://lekalo.dev/contracts/${family}.schema.v1.0.0.json`,...body,
}]));
