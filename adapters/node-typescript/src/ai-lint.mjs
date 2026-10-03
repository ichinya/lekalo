// Issue #76: bounded static evidence. No project imports or execution.
import { createHash } from 'node:crypto';
const unknown = () => ({ state: 'unknown' });
const known = value => ({ state: 'known', value });
const canon = v => JSON.stringify(order(v));
function order(v) { if (Array.isArray(v)) return v.map(order); if (v && typeof v === 'object') return Object.fromEntries(Object.keys(v).sort().map(k => [k, order(v[k])])); return v; }
const hash = v => 'sha256:' + createHash('sha256').update(canon(v)).digest('hex');
const digest = bytes => 'sha256:' + createHash('sha256').update(bytes).digest('hex');
const rules = ['ambiguity.implicit-target-defaults','ambiguity.multiple-resolutions','ambiguity.scattered-state-writes','hidden.convention-only-path','hidden.dispatch-without-binding','hidden.observer-write','hidden.path-without-trace-owner','hidden.reflective-call','hidden.string-reference','hidden.undeclared-effect','indirection.depth-exceeded'];
export function validateLintRequest(request) {
  const l = request.lint_request;
  if ((request.operation === 'lint') !== (l !== undefined)) throw new Error('lint-request-pairing');
  if (!l) return;
  if (request.protocol_version !== '0.6.4' || !request.target || ['dry_run','plan_id','native_request','ir_path','profile','profile_digest','profile_capabilities'].some(k => request[k] !== undefined)) throw new Error('lint-request-version-members');
  if (!l || Object.keys(l).sort().join() !== 'bindings,files,pins,scope' || !Array.isArray(l.scope) || !l.scope.length || l.scope.length > 10000 || !Array.isArray(l.files) || l.files.length > 4096 || !Array.isArray(l.bindings) || l.bindings.length > 10000) throw new Error('lint-request-shape');
  const token = s => typeof s === 'string' && /^[A-Za-z0-9][A-Za-z0-9_.:/#@\\-]{0,255}$/.test(s);
  const sha = s => typeof s === 'string' && /^sha256:[a-f0-9]{64}$/.test(s);
  if (l.scope.some((s,i) => !token(s) || i > 0 && l.scope[i-1] >= s) || l.files.some((s,i) => !/^[a-z0-9._/-]{1,512}$/.test(s) || s.split('/').some(p => !p || p === '.' || p === '..') || i > 0 && l.files[i-1] >= s)) throw new Error('lint-request-order-path');
  if (Object.keys(l.pins).sort().join() !== 'capabilities,ir,model,observed,profile,revision') throw new Error('lint-request-pins');
  for (const [key,p] of Object.entries(l.pins)) {
    if (!p || !['known','unknown','withheld','unsupported'].includes(p.state) || Object.keys(p).sort().join() !== (p.state === 'known' ? 'state,value' : 'state') || p.state === 'known' && !(key === 'revision' ? token(p.value) : sha(p.value)) || ['model','ir'].includes(key) && p.state !== 'known') throw new Error('lint-request-pin');
  }
  const keys=new Set(); for(const b of l.bindings) {const key=JSON.stringify([b.path,b.nativeId,b.kind]);if(keys.has(key))throw new Error('lint-binding-ambiguous');keys.add(key);}
  for (const b of l.bindings) if (!b || Object.keys(b).sort().join() !== 'fingerprint,kind,nativeId,path,symbol' || !l.files.includes(b.path) || !token(b.nativeId) || !token(b.symbol) || !sha(b.fingerprint) || !['command','entity'].includes(b.kind)) throw new Error('lint-request-binding');
}
export function collectAiLint(request, ts, readView, artifactDigest, adapterVersion) {
  const l = request.lint_request, texts = new Map(), sources = [], locations = [], records = [];
  let total = 0, work = 0;
  for (const path of l.files) {
    const bytes = readView.readFile(path, { files: 4096, bytes: 8 * 1024 * 1024 });
    total += bytes.length; if (total > 8 * 1024 * 1024) throw new Error('lint-source-limit');
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes), fingerprint = digest(bytes);
    texts.set(path, text); sources.push({ id: hash([path,fingerprint]), path, fingerprint, bytes: bytes.length });
  }
  for (const b of l.bindings) if (sources.find(s => s.path === b.path)?.fingerprint !== b.fingerprint) throw new Error('lint-binding-stale');
  const files = new Map([...texts].map(([path,text]) => [path, ts.createSourceFile(path,text,ts.ScriptTarget.ES2022,true,path.endsWith('.tsx') ? ts.ScriptKind.TSX : path.endsWith('.js') ? ts.ScriptKind.JS : ts.ScriptKind.TS)]));
  const host = { getSourceFile: path => files.get(path), getDefaultLibFileName: () => '', writeFile: () => { throw new Error('lint-write-denied'); }, getCurrentDirectory: () => '', getDirectories: () => [], fileExists: path => files.has(path), readFile: path => texts.get(path), getCanonicalFileName: path => path, useCaseSensitiveFileNames: () => true, getNewLine: () => '\n', directoryExists: () => false, realpath: path => path, resolveModuleNames: names => names.map(() => undefined) };
  const program = ts.createProgram([...files.keys()], { noLib: true, noEmit: true, allowJs: true, module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 }, host), checker = program.getTypeChecker();
  const position = (text, offset) => { const pre = text.slice(0,offset); return [pre.split('\n').length, Array.from(pre.split('\n').at(-1)).length+1]; };
  function span(node, sf) {
    const source = sources.find(s => s.path === sf.fileName), start = node.getStart(sf), end = node.end;
    const startByte = Buffer.byteLength(sf.text.slice(0,start)), endByte = Buffer.byteLength(sf.text.slice(0,end));
    const id = hash([source.id,startByte,endByte]);
    if (!locations.some(s => s.id === id)) { const [line,column] = position(sf.text,start), [endLine,endColumn] = position(sf.text,end); locations.push({ id, source: source.id, start: startByte, end: endByte, line,column,endLine,endColumn }); }
    return id;
  }
  function native(node, sf) {
    let parts = [], at = node;
    while (at && at !== sf) { if (at.name && (ts.isFunctionDeclaration(at) || ts.isClassDeclaration(at) || ts.isMethodDeclaration(at))) parts.unshift(at.name.text); at = at.parent; }
    return `${sf.fileName}#${parts.join('.') || 'module'}`;
  }
  const binding = (node,sf,kind) => l.bindings.find(b => b.path === sf.fileName && b.nativeId === native(node,sf) && b.kind === kind);
  function base(kind, subject, nativeId, ids, symbol = unknown()) {
    const r = { id: '', kind, mechanism: 'direct', subject, semanticSymbol: symbol, operation: unknown(), resource: unknown(), field: unknown(), nativeId, confidence: 'medium', currency: 'current', origin: 'extracted', claim: 'possible-behavior', binding: unknown(), configuration: unknown(), ownership: unknown(), trace: unknown(), value: unknown(), key: unknown(), candidates: [], activation: [], locations: [...new Set(ids)].sort(), guards: [] };
    r.id = hash([kind,subject,nativeId,r.locations]); return r;
  }
  function push(r) { if (records.length >= 10000 || locations.length > 10000) throw new Error('lint-record-limit'); if (!records.some(s => s.id === r.id)) records.push(r); }
  function enclosing(node,sf) { let n = node; while (n && n !== sf) { if (ts.isFunctionDeclaration(n) || ts.isMethodDeclaration(n)) return n; n = n.parent; } return sf; }
  function write(node,sf) {
    if (!ts.isBinaryExpression(node) || node.operatorToken.kind !== ts.SyntaxKind.EqualsToken || !ts.isPropertyAccessExpression(node.left)) return null;
    const type = checker.getTypeAtLocation(node.left.expression), declaration = type.symbol?.declarations?.find(d => ts.isClassDeclaration(d));
    if (!declaration) return null;
    const resource = binding(declaration,declaration.getSourceFile(),'entity');
    if (!resource) return null;
    return { resource: resource.symbol, field: node.left.name.text, location: span(node,sf) };
  }
  const emitters = new Set(), registrations = [], triggers = [], calls = [];
  function walk(node,sf,visit) { const stack=[node]; while(stack.length) { const n=stack.pop(); if (++work > 10000000) throw new Error('lint-work-limit'); visit(n); ts.forEachChild(n,child => {stack.push(child);}); } }
  for (const sf of files.values()) walk(sf,sf,node => {
    if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer && ts.isNewExpression(node.initializer)) {
      const sym = checker.getSymbolAtLocation(node.initializer.expression), imports = sym?.declarations?.filter(d => ts.isImportSpecifier(d)) ?? [];
      if (imports.some(d => (d.propertyName?.text ?? d.name.text) === 'EventEmitter' && ['node:events','events'].includes(d.parent.parent.parent.moduleSpecifier?.text))) emitters.add(checker.getSymbolAtLocation(node.name));
    }
    if (ts.isCallExpression(node)) calls.push([node,sf]);
    const w = write(node,sf), owner = enclosing(node,sf), bound = binding(owner,sf,'command');
    if (w && bound && l.scope.some(s => s === bound.symbol || bound.symbol.startsWith(s + '.'))) {
      const r = base('field-write',bound.symbol,native(owner,sf),[w.location],known(bound.symbol)); Object.assign(r,{ operation: known(bound.symbol), resource: known(w.resource), field: known(w.field), key: known('update'), confidence: 'high', claim: 'structural' }); push(r);
    }
  });
  // A mutable event-method slot invalidates the recognized API identity.
  for (const sf of files.values()) walk(sf,sf,node => { if (ts.isBinaryExpression(node) && ts.isPropertyAccessExpression(node.left) && ['on','emit','addListener','once'].includes(node.left.name.text)) emitters.delete(checker.getSymbolAtLocation(node.left.expression)); });
  for (const [call,sf] of calls) {
    const owner = enclosing(call,sf), bound = binding(owner,sf,'command'), symbol = bound ? known(bound.symbol) : unknown(), subject = bound?.symbol ?? native(owner,sf);
    if (bound && !l.scope.some(s => s === bound.symbol || bound.symbol.startsWith(s + '.'))) continue;
    const expr = call.expression;
    if (ts.isElementAccessExpression(expr)) {
      const resolved = ts.isStringLiteral(expr.argumentExpression) && checker.getResolvedSignature(call)?.declaration;
      if (!resolved) { const r = base('reflection',subject,native(owner,sf)+':computed-call',[span(call,sf)],symbol); r.binding = known(false); if (bound) r.operation = known(bound.symbol); push(r); }
    }
    if (!ts.isPropertyAccessExpression(expr) || !emitters.has(checker.getSymbolAtLocation(expr.expression))) continue;
    const bus = checker.getSymbolAtLocation(expr.expression), event = call.arguments[0];
    if (!event || !ts.isStringLiteral(event)) { const r = base('string-reference',subject,native(owner,sf)+':event-reference',[span(call,sf)],symbol); r.binding = known(false); push(r); continue; }
    if (['on','addListener','once'].includes(expr.name.text)) {
      const callbackArg = call.arguments[1], callback = callbackArg && (ts.isArrowFunction(callbackArg) || ts.isFunctionExpression(callbackArg) ? callbackArg : checker.getSymbolAtLocation(callbackArg)?.declarations?.find(d => ts.isFunctionDeclaration(d)));
      if (callback?.body) registrations.push({ bus, event: event.text, call, callback, sf, once: expr.name.text === 'once' });
    }
    if (expr.name.text === 'emit' && bound) triggers.push({bus,event:event.text,call,sf,bound,owner});
  }
  for (const trigger of triggers) for (const registration of registrations.filter(r => r.bus === trigger.bus && r.event === trigger.event)) {
    const callbackFile = registration.callback.getSourceFile();
    walk(registration.callback.body,callbackFile,node => {
      const w = write(node,callbackFile); if (!w) return;
      const triggerSpan = span(trigger.call,trigger.sf), registrationSpan = span(registration.call,registration.sf), callbackSpan = span(registration.callback,callbackFile);
      const r = base('effect',trigger.bound.symbol,native(trigger.owner,trigger.sf)+':observer:'+hash(registration.event).slice(7,23),[triggerSpan,registrationSpan,callbackSpan,w.location],known(trigger.bound.symbol));
      Object.assign(r,{ operation: known(trigger.bound.symbol), resource: known(w.resource), field: known(w.field), key: known('update'), confidence: 'high', mechanism: 'observer' });
      r.activation = [ ['binding',trigger.bound.nativeId,triggerSpan], ['trigger',native(trigger.owner,trigger.sf),triggerSpan], ['registration',native(enclosing(registration.call,registration.sf),registration.sf)+':on',registrationSpan], ['callback',native(registration.callback,callbackFile)+':callback',callbackSpan], ['effect',w.resource+'/'+w.field,w.location] ].map(([role,identity,location]) => ({ role,identity,locations:[location],guards: registration.once ? ['once-listener','listener-still-registered'] : ['listener-still-registered'],confidence:'high' }));
      push(r);
    });
  }
  sources.sort((a,b) => a.path < b.path ? -1 : 1); locations.sort((a,b) => a.id < b.id ? -1 : 1); records.sort((a,b) => a.id < b.id ? -1 : 1);
  const supported = new Set(['hidden.observer-write','hidden.reflective-call','hidden.string-reference','ambiguity.scattered-state-writes']);
  return { schemaVersion: 'lekalo/ai-lint-evidence/v0.6.4', identity: 'dev.lekalo.ai-lint-evidence@0.6.4', target: request.target, scope: l.scope, producer: { id: 'lekalo-target-node-typescript', version: adapterVersion, artifactDigest, tool: 'node-static', compiler: known('typescript/5.9.3'), framework: known('node-events/v1'), recipe: 'ai-readability/1' }, pins: l.pins, inputManifestDigest: hash(sources), sources, locations, records, limitations: ['static-files-only','no-application-execution','external-module-resolution-unsupported'], coverage: rules.flatMap(rule => l.scope.map(scope => ({ rule,target:request.target,scope,state:supported.has(rule) && files.size ? 'partial' : 'unsupported',eligible:unknown(),examined:known(calls.length),limitations:[supported.has(rule)?'bounded-static-files':'detector-unsupported'] }))) };
}
