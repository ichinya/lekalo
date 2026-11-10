/** TypeScript 7 bridge. All syntax and semantic objects come from the native
 * compiler; the host supplies a closed VFS and owns the compiler process. */
import * as native from './compiler-client.mjs';
import {posix} from 'node:path';

const logical = name => String(name).replaceAll('\\', '/').replace(/^[a-z]:\/(?=lekalo\/)/i, '/');
const absolute = name => logical(name).startsWith('/') ? logical(name) : `/lekalo/project/${logical(name)}`;
let hostSession = null;

export function configureCompilerSession(argv) {
  const positions = argv.flatMap((value, index) => value === '--lekalo-compiler-session-json' ? [index] : []);
  if (positions.length === 0) return null;
  if (positions.length !== 1) throw new Error('ambiguous compiler session');
  const value = JSON.parse(argv[positions[0] + 1]);
  const resource = native.compilerComponents.resources.find(row => row.platform === `${process.platform}-${process.arch}`);
  if (!resource || Object.keys(value).sort().join(',') !== 'digest,kind,libraries_digest,platform,schema_version,version'
    || value.schema_version !== 'lekalo/compiler-runtime/v1.3.0' || value.kind !== 'typescript-native'
    || value.version !== native.compilerComponents.typescript || value.platform !== resource.platform
    || value.digest !== `sha256:${resource.sha256}` || value.libraries_digest !== `sha256:${native.compilerComponents.standardLibraries.sha256}`) throw new Error('compiler session integrity');
  return hostSession = Object.freeze(value);
}

/** The API factory is injectable for source tests. Production never spawns. */
export function createCompiler(apiFactory = options => {
  if (!hostSession) throw new Error('TypeScript 7 requires the core-supervised runtime bundle');
  return new native.API(options);
}) {
  let api, host = null, sequence = 0;
  const overlay = new Map(), snapshots = [], sourceCache = new Map();
  const key = name => absolute(name).toLowerCase();
  const fs = {
    readFile(name) {
      const path = logical(name), own = overlay.get(key(path));
      if (own !== undefined) return own;
      const lib = path.split('/').at(-1);
      if (!path.startsWith('/lekalo/project/') && /^lib(\..+)?\.d\.ts$/.test(lib)) return host?.readFile(`/lekalo/libs/${lib}`) ?? null;
      const hostPath = host?.getCurrentDirectory?.() === '' ? path.replace(/^\/lekalo\/project\//, '') : path;
      return host?.readFile(hostPath) ?? null; // undefined MUST NOT reach TS7 (real-FS fallback).
    },
    fileExists: name => fs.readFile(name) !== null,
    directoryExists(name) {
      const prefix = key(name).replace(/\/$/, '') + '/';
      return [...overlay.keys()].some(path => path.startsWith(prefix)) || Boolean(host?.directoryExists(logical(name)));
    },
    getAccessibleEntries(name) {
      const prefix = key(name).replace(/\/$/, '') + '/', files = new Set(), directories = new Set();
      const paths = [...overlay.keys(), ...(host?.readDirectory?.(logical(name), undefined, undefined, undefined) ?? [])];
      for (const path of paths) {
        const normalized = key(path);
        if (!normalized.startsWith(prefix)) continue;
        const tail = normalized.slice(prefix.length), slash = tail.indexOf('/');
        if (slash < 0) files.add(tail); else directories.add(tail.slice(0, slash));
      }
      return {files: [...files].sort(), directories: [...directories].sort()};
    },
    realpath: name => absolute(name),
  };
  const client = () => api ??= apiFactory({tsserverPath: 'lekalo-core-owned-compiler', cwd: '/lekalo/project', fs});
  const sourceFile = (program, name) => {
    const source = program.getSourceFile(name);
    if (source && source.fileName !== logical(source.fileName)) {
      Object.defineProperty(source, 'fileName', {value: logical(source.fileName), configurable: true});
    }
    return source;
  };
  const diagnostic = (program, value) => ({
    ...value, file: value.fileName ? sourceFile(program, value.fileName) : undefined,
    start: value.pos, length: value.end === undefined ? undefined : value.end - value.pos,
    messageText: value.text,
  });
  const open = (config, content) => {
    overlay.set(key(config), JSON.stringify(content));
    const snapshot = client().updateSnapshot({openProjects: [config], fileChanges: {invalidateAll: true}});
    snapshots.push(snapshot);
    const project = snapshot.getProject(config);
    if (!project) throw new Error('native compiler did not open the admitted project');
    return project;
  };
  const jsonOptions = options => {
    const result = {...options};
    for (const [name, values] of Object.entries({target: native.ast.ScriptTarget, module: native.ModuleKind,
      moduleResolution: native.ModuleResolutionKind, jsx: native.JsxEmit, newLine: native.NewLineKind,
      moduleDetection: native.ModuleDetectionKind})) {
      if (typeof result[name] === 'number') {
        const text = values[result[name]];
        if (!text) throw new Error(`unsupported native compiler option ${name}`);
        result[name] = name === 'newLine' ? (result[name] === native.NewLineKind.LineFeed ? 'lf' : 'crlf')
          : ({latest: 'esnext', reactjsx: 'react-jsx', reactjsxdev: 'react-jsxdev'}[text.toLowerCase()] ?? text.toLowerCase());
      }
    }
    if (Array.isArray(result.lib)) result.lib = result.lib.map(name => name.replace(/^lib\./, '').replace(/\.d\.ts$/, ''));
    // This is native parse bookkeeping, not a compiler option.
    delete result.configFilePath;
    delete result.pathsBasePath;
    return result;
  };
  const ts = {
    ...native.ast, ...native,
    version: native.compilerComponents.typescript,
    __setApiFactoryForTests(factory) {
      if (api) throw new Error('cannot replace an active compiler session');
      apiFactory = factory;
    },
    ModuleResolutionKind: native.ModuleResolutionKind,
    IndexKind: Object.freeze({String: 0, Number: 1}),
    TypeFormatFlags: native.NodeBuilderFlags,
    forEachChild: (node, callback) => node.forEachChild(callback),
    getCombinedModifierFlags(node) {
      let flags = 0;
      const kinds = {ExportKeyword: 'Export', DeclareKeyword: 'Ambient', PublicKeyword: 'Public',
        PrivateKeyword: 'Private', ProtectedKeyword: 'Protected', StaticKeyword: 'Static',
        ReadonlyKeyword: 'Readonly', AbstractKeyword: 'Abstract', AsyncKeyword: 'Async',
        DefaultKeyword: 'Default', ConstKeyword: 'Const', OverrideKeyword: 'Override'};
      for (const modifier of node.modifiers ?? []) flags |= native.ast.ModifierFlags[kinds[native.ast.SyntaxKind[modifier.kind]]] ?? 0;
      if (node.questionToken) flags |= native.ast.ModifierFlags.Optional ?? 0;
      return flags;
    },
    getJSDocCommentsAndTags: node => node.jsDoc ?? [],
    getDefaultLibFileName(options) {
      const name = native.ast.ScriptTarget[options.target ?? native.ast.ScriptTarget.ESNext]?.toLowerCase();
      return name === 'es5' || name === 'es3' ? 'lib.d.ts' : `lib.${name === 'latest' ? 'esnext' : name}.full.d.ts`;
    },
    flattenDiagnosticMessageText(value, separator) {
      if (typeof value === 'string') return value;
      return [value?.messageText ?? '', ...(value?.next ?? []).map(part => ts.flattenDiagnosticMessageText(part, separator))].join(separator);
    },
    readConfigFile(name, read) {
      try { return {config: parseJsonConfig(read(name), native.ast)}; }
      catch (error) { return {error: {code: 1005, category: native.DiagnosticCategory.Error, messageText: error.message}}; }
    },
    parseJsonConfigFileContent(raw, configHost, directory, overrides, configName) {
      host = configHost;
      const config = absolute(configName ?? `${directory}/tsconfig.json`);
      overlay.set(key(config), JSON.stringify({...raw, compilerOptions: {...raw.compilerOptions, ...overrides}}));
      const parsed = client().parseConfigFile(config);
      // Numeric native options can be merged across configs, but relative
      // path mappings retain the declaring config's base directory.
      if (parsed.options.paths) {
        const base = parsed.options.pathsBasePath ? logical(parsed.options.pathsBasePath)
          : posix.dirname(logical(parsed.options.configFilePath ?? config));
        parsed.options.paths = Object.fromEntries(Object.entries(parsed.options.paths).map(([name, paths]) =>
          [name, paths.map(path => posix.isAbsolute(path) ? path : posix.resolve(base, path))]));
      }
      const project = open(config, {...raw, compilerOptions: {...raw.compilerOptions, ...overrides}});
      const errors = [...project.program.getConfigFileParsingDiagnostics(), ...project.program.getProgramDiagnostics()]
        .map(value => diagnostic(project.program, value));
      return {fileNames: parsed.fileNames.map(logical), options: parsed.options, errors};
    },
    createSourceFile(name, text, target, _parents, scriptKind) {
      const path = absolute(name), cached = sourceCache.get(path);
      if (cached?.text === text) return cached;
      overlay.set(key(path), text);
      const project = open(`/lekalo/project/__lekalo_parse_${++sequence}.json`, {
        compilerOptions: {noLib: true, noEmit: true, allowJs: true, target: 'esnext',
          module: 'esnext', moduleResolution: 'bundler', resolveJsonModule: path.endsWith('.json')}, files: [path],
      });
      const source = sourceFile(project.program, path);
      if (!source) throw new Error('native compiler did not parse the admitted source');
      if (!logical(name).startsWith('/')) Object.defineProperty(source, 'fileName', {value: logical(name), configurable: true});
      sourceCache.set(path, source);
      return source;
    },
    createProgram(input, legacyOptions, legacyHost) {
      const {rootNames, options, host: compilerHost} = Array.isArray(input)
        ? {rootNames: input, options: legacyOptions, host: legacyHost} : input;
      host = compilerHost;
      const project = open(`/lekalo/project/__lekalo_scan_${++sequence}.json`, {
        compilerOptions: jsonOptions(options), files: rootNames.map(absolute),
      });
      const program = project.program, checker = legacyChecker(project, logical);
      checker.getIndexTypeOfType = (type, kind) => checker.getIndexInfosOfType(type).find(info =>
        info.keyType.flags & (kind === 0 ? native.TypeFlags.String : native.TypeFlags.Number))?.type;
      const result = {
        getCompilerOptions: () => options,
        getSourceFiles: () => program.getSourceFileNames().map(name => sourceFile(program, name)),
        getSourceFile: name => sourceFile(program, absolute(name)),
        getTypeChecker: () => checker,
        isSourceFileDefaultLibrary: file => program.isSourceFileDefaultLibrary(file),
        isSourceFileFromExternalLibrary: file => program.isSourceFileFromExternalLibrary(file),
        getResolvedModuleFromModuleSpecifier(specifier) {
          const symbol = checker.getSymbolAtLocation(specifier), declaration = symbol?.declarations?.find(node => node.kind === native.ast.SyntaxKind.SourceFile);
          return declaration ? {resolvedModule: {resolvedFileName: logical(declaration.fileName), isExternalLibraryImport: false}} : undefined;
        },
        getResolvedModule: () => undefined,
      };
      for (const method of ['getSyntacticDiagnostics', 'getSemanticDiagnostics', 'getGlobalDiagnostics', 'getConfigFileParsingDiagnostics']) {
        result[method] = file => program[method](file?.path ?? file).map(value => diagnostic(program, value));
      }
      result.getOptionsDiagnostics = () => program.getProgramDiagnostics().map(value => diagnostic(program, value));
      return result;
    },
    close() {
      for (const snapshot of snapshots.splice(0)) snapshot.dispose();
      api?.close(); api = undefined; sourceCache.clear(); overlay.clear(); host = null;
    },
  };
  return ts;
}

/** Resolve native handles lazily while retaining identity and the original
 * objects for RPC. Native methods always receive their own unwrapped objects. */
function legacyChecker(project, logical) {
  const proxies = new WeakMap(), originals = new WeakMap();
  const normalizeNode = node => {
    const file = node?.getSourceFile?.();
    if (file && file.fileName !== logical(file.fileName)) Object.defineProperty(file, 'fileName', {value: logical(file.fileName), configurable: true});
    return node;
  };
  const unwrap = value => Array.isArray(value) ? value.map(unwrap) : originals.get(value) ?? value;
  const wrap = value => {
    if (!value || typeof value !== 'object') return value;
    if (typeof value.resolve === 'function' && typeof value.path === 'string') return normalizeNode(value.resolve());
    if (typeof value.kind === 'number' && value.forEachChild) return normalizeNode(value);
    if (Array.isArray(value)) return value.map(wrap);
    if (value instanceof Map) return new Map([...value].map(([key, item]) => [key, wrap(item)]));
    if (proxies.has(value)) return proxies.get(value);
    const proxy = new Proxy(value, {get(raw, property) {
      if (property === 'getName') return () => raw.name;
      if (property === 'getFlags') return () => raw.flags;
      if (property === 'getDeclarations') return () => wrap(raw.declarations);
      if (property === 'getProperties' && raw.isObjectType) return () => wrap(project.checker.getPropertiesOfType(raw));
      const fieldMethods = {symbol: 'getSymbol', aliasSymbol: 'getAliasSymbol', parent: 'getParent',
        target: 'getTarget', types: 'getTypes', parameters: 'getParameters', typeParameters: 'getTypeParameters',
        aliasTypeArguments: 'getAliasTypeArguments', thisParameter: 'getThisParameter', members: 'getMembers', exports: 'getExports'};
      if (fieldMethods[property] && typeof raw[fieldMethods[property]] === 'function') return wrap(raw[fieldMethods[property]]());
      if (property === 'typeArguments' && raw.isTypeReference?.()) return wrap(project.checker.getTypeArguments(raw));
      if (property === 'isUnion') return () => raw.isUnionType();
      if (property === 'isIntersection') return () => raw.isIntersectionType();
      if (property === 'isStringLiteral') return () => raw.isStringLiteralType();
      if (property === 'isNumberLiteral') return () => raw.isNumberLiteralType();
      if (property === 'getDocumentationComment') return checker => [{text: raw.getDocumentationComment(unwrap(checker))}];
      const result = Reflect.get(raw, property, raw);
      return typeof result === 'function' ? (...args) => wrap(result.apply(raw, args.map(unwrap))) : wrap(result);
    }});
    proxies.set(value, proxy); originals.set(proxy, value); return proxy;
  };
  return wrap(project.checker);
}

/** JSONC parser over the pinned TS7 scanner. No evaluation, regex comment
 * removal, project imports, prototype mutation or ambient filesystem. */
export function parseJsonConfig(text, ast) {
  if (typeof text !== 'string' || text.length > 4 * 1024 * 1024) throw new Error('invalid config size');
  let token, count = 0;
  const scanner = ast.createScanner(false, ast.LanguageVariant.Standard, text), kind = ast.SyntaxKind;
  const next = () => {
    do {
      token = scanner.scan();
      if (token === kind.MultiLineCommentTrivia && !scanner.getTokenText().endsWith('*/')) throw new Error('unterminated config comment');
    } while ([kind.WhitespaceTrivia, kind.NewLineTrivia, kind.SingleLineCommentTrivia, kind.MultiLineCommentTrivia].includes(token));
    return token;
  };
  const string = () => JSON.parse(scanner.getTokenText());
  const require = expected => { if (token !== expected) throw new Error('invalid JSON config'); next(); };
  const value = depth => {
    if (depth > 64 || ++count > 100000) throw new Error('config structure limit');
    if (token === kind.OpenBraceToken || token === kind.OpenBracketToken) {
      const object = token === kind.OpenBraceToken, end = object ? kind.CloseBraceToken : kind.CloseBracketToken;
      const result = object ? Object.create(null) : []; next();
      while (token !== end) {
        let name;
        if (object) { if (token !== kind.StringLiteral) throw new Error('config key must be a string'); name = string(); next(); require(kind.ColonToken); }
        const item = value(depth + 1);
        if (object) result[name] = item; else result.push(item);
        if (token === end) break;
        require(kind.CommaToken);
      }
      next(); return result;
    }
    if (token === kind.StringLiteral) { const result = string(); next(); return result; }
    if (token === kind.NumericLiteral || token === kind.MinusToken) {
      const negative = token === kind.MinusToken; if (negative) next();
      if (token !== kind.NumericLiteral) throw new Error('invalid config number');
      const raw = scanner.getTokenText();
      if (!/^(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?$/.test(raw)) throw new Error('invalid config number');
      const result = Number(raw) * (negative ? -1 : 1); next(); return result;
    }
    for (const [type, result] of [[kind.TrueKeyword, true], [kind.FalseKeyword, false], [kind.NullKeyword, null]]) {
      if (token === type) { next(); return result; }
    }
    throw new Error('invalid config value');
  };
  next(); const result = value(0);
  if (token !== kind.EndOfFile || !result || Array.isArray(result) || typeof result !== 'object') throw new Error('config must be one object');
  return result;
}
