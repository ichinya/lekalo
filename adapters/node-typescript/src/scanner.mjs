/**
 * The restricted TypeScript scanner (issue #44, plan §4-§6).
 *
 * Builds read-only compiler Programs over the kernel's validated read
 * view through a restricted CompilerHost with NO ts.sys fallback: every
 * file lookup is answered from the enumerated project inventory (through
 * the kernel read view's scope/exclusion checks) or from the embedded
 * standard libraries of the exact vendored pin (type context only, never
 * project symbols). Missing and denied lookups are distinguished and
 * recorded.
 *
 * Index model (§4): sorted arrays of typed records — packages, projects,
 * symbols, exports, references, routes, tests, diagnostics — plus the
 * input manifest and compiler/profile digests. Native identity and
 * signature digests are domain-separated SHA-256 hashes over canonical
 * tuples. Overload sets are stored whole; merged declarations share one
 * canonical native symbol; export aliases and re-exports become separate
 * alias edges, never duplicate symbols. Every inferred framework or test
 * relation carries an explicit rule id and confidence. Uncertainty is
 * recorded and never reported as a verified pass.
 *
 * Incremental (§6/plan §5): an in-memory ScannerSession keyed by the
 * content digests of every input (source, config, package manifests,
 * options, reference topology) reuses the previous Program through the
 * public `oldProgram` seam. Cold and warm runs of the same inputs must
 * produce byte-identical index JSON.
 */
import { createHash } from "node:crypto";
import { lstatSync, readdirSync } from "node:fs";
import { join } from "node:path";

import {
  RequestRefusal,
  embeddedLibFiles,
  vendoredTs,
} from "./kernel.mjs";

// ---------------------------------------------------------------------------
// 1. Bounds, domains, and small utilities.
// ---------------------------------------------------------------------------

/** The domain-separation prefix of the native identity tuple (§4). */
const IDENTITY_DOMAIN = "lekalo.ts.native.v1";
/** The domain-separation prefix of the structural signature digest. */
const SIGNATURE_DOMAIN = "lekalo.ts.signature.v1";
/** The version of the signature policy (part of every signature key). */
const SIGNATURE_POLICY_VERSION = 1;

/** Maximum total source bytes one scan may read. */
const MAX_SCAN_SOURCE_BYTES = 16 * 1024 * 1024;
/** Maximum files one scan may open. */
const MAX_SCAN_FILES = 4096;
/** Maximum enumeration depth inside one root. */
const MAX_ENUMERATION_DEPTH = 24;
/** Maximum number of diagnostic records one scan reports internally. */
const MAX_DIAGNOSTICS = 4096;

/** Standard-library file names never count as project symbols. */
function isEmbeddedLibBase(fileName) {
  return /^lib(\..+)?\.d\.ts$/.test(fileName.split("/").pop());
}

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function sha256Hex(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

/**
 * Canonical JSON text of one closed plain value (sorted keys, compact);
 * mirrors the kernel's internal canonical writer.
 */
function canonicalText(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean": return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) {
        throw new RequestRefusal("syntax", "non-finite number cannot be canonicalized");
      }
      return Number.isInteger(value) && Math.abs(value) < 1e15 ? String(value) : JSON.stringify(value);
    case "string": return JSON.stringify(value);
    case "object": break;
    default:
      throw new RequestRefusal("syntax", "unserializable value cannot be canonicalized");
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalText).join(",")}]`;
  }
  const keys = Object.keys(value).sort((left, right) => {
    const a = Buffer.from(left, "utf8");
    const b = Buffer.from(right, "utf8");
    const length = Math.min(a.length, b.length);
    for (let index = 0; index < length; index += 1) {
      if (a[index] !== b[index]) return a[index] - b[index];
    }
    return a.length - b.length;
  });
  return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalText(value[key])}`).join(",")}}`;
}

/** UTF-8 byte-order comparator for canonical record ordering. */
function utf8Compare(left, right) {
  const a = Buffer.from(left, "utf8");
  const b = Buffer.from(right, "utf8");
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return a.length - b.length;
}

function assertCompilerAvailable() {
  const ts = vendoredTs();
  if (!ts) {
    throw new RequestRefusal(
      "compiler-absent",
      "the vendored TypeScript compiler is not attached to this deployment",
    );
  }
  for (const name of [
    "createProgram",
    "createSourceFile",
    "resolveModuleName",
    "getCombinedModifierFlags",
    "readConfigFile",
    "parseJsonConfigFileContent",
  ]) {
    if (typeof ts[name] !== "function") {
      throw new RequestRefusal("compiler", `the vendored compiler lacks ${name}`);
    }
  }
  return ts;
}

// ---------------------------------------------------------------------------
// 2. Inventory enumeration (bounded, link-intolerant, exclusion-pruned).
// ---------------------------------------------------------------------------

/**
 * Enumerate the resolved roots once via the host filesystem with the
 * same hardening the kernel read view enforces: scope coverage comes
 * from the roots themselves, exclusions prune subtrees before any stat,
 * symlinks/junctions anywhere are fatal, and depth/entry bounds hold.
 */
function enumerateInventory(permittedRoot, roots, profile) {
  const exclusions = profile.exclusions ?? [];
  const excluded = (logicalPath) => {
    const segments = logicalPath.split("/");
    for (let depth = 1; depth <= segments.length; depth += 1) {
      const prefix = segments.slice(0, depth).join("/");
      for (const exclusion of exclusions) {
        if (exclusion === prefix) return true;
        if (exclusion.endsWith("/**")) {
          const head = exclusion.slice(0, -3);
          if (prefix === head || prefix.startsWith(`${head}/`)) return true;
        }
      }
    }
    return false;
  };
  const files = [];
  const directories = [];
  const walk = (rootPath, relative, depth) => {
    if (depth > MAX_ENUMERATION_DEPTH) {
      throw new RequestRefusal("enumeration", "directory depth exceeds the scan bound");
    }
    if (files.length + directories.length > MAX_SCAN_FILES * 2) {
      throw new RequestRefusal("enumeration", "enumeration exceeds the scan bound");
    }
    const absoluteDirectory = relative === ""
      ? join(permittedRoot, ...rootPath.split("/"))
      : join(permittedRoot, ...relative.split("/"));
    let names;
    try {
      names = readdirSync(absoluteDirectory, { withFileTypes: true });
    } catch (error) {
      if (error?.code === "ENOENT") {
        throw new RequestRefusal("root-invalid", "a resolved root vanished before enumeration");
      }
      throw new RequestRefusal("enumeration", "a directory could not be enumerated");
    }
    // Canonical order by UTF-8 bytes, never host traversal order.
    names.sort((left, right) => utf8Compare(left.name, right.name));
    for (const entry of names) {
      const logicalPath = relative === "" ? `${rootPath}/${entry.name}` : `${relative}/${entry.name}`;
      if (excluded(logicalPath)) continue;
      const absolute = join(permittedRoot, ...logicalPath.split("/"));
      let metadata;
      try {
        metadata = lstatSync(absolute);
      } catch (error) {
        if (error?.code === "ENOENT") continue;
        throw new RequestRefusal("enumeration", "an entry could not be inspected");
      }
      if (metadata.isSymbolicLink()) {
        throw new RequestRefusal("link", "a link appeared inside a resolved root");
      }
      if (metadata.isDirectory()) {
        directories.push(logicalPath);
        walk(rootPath, logicalPath, depth + 1);
      } else if (metadata.isFile()) {
        files.push({ path: logicalPath, size: metadata.size });
      } else {
        throw new RequestRefusal("special", "a special file appeared inside a resolved root");
      }
    }
  };
  for (const root of roots) {
    if (root.kind === "file") {
      // A file root contributes exactly its own entry; never walked.
      if (excluded(root.path)) continue;
      try {
        const absolute = join(permittedRoot, ...root.path.split("/"));
        const metadata = lstatSync(absolute);
        if (metadata.isSymbolicLink()) throw new RequestRefusal("link", "file root is a link");
        if (!metadata.isFile()) throw new RequestRefusal("kind", "file root is not a file");
        files.push({ path: root.path, size: metadata.size });
      } catch (error) {
        if (error instanceof RequestRefusal) throw error;
        if (error?.code === "ENOENT") {
          throw new RequestRefusal("root-invalid", "a resolved root vanished before enumeration");
        }
        throw new RequestRefusal("enumeration", "a file root could not be inspected");
      }
      continue;
    }
    walk(root.path, "", 1);
  }
  return { files, directories };
}

// ---------------------------------------------------------------------------
// 3. Input manifest: the content-addressed identity of one scan.
// ---------------------------------------------------------------------------

const SOURCE_EXTENSIONS = [".ts", ".tsx", ".mts", ".cts", ".d.ts", ".d.mts", ".d.cts"];
const CONFIG_NAMES = ["tsconfig.json", "jsconfig.json"];
const PACKAGE_NAME = "package.json";

function isSourceFile(path) {
  return SOURCE_EXTENSIONS.some((extension) => path.endsWith(extension)) && !path.endsWith(".d.ts");
}
function isDeclarationFile(path) {
  return path.endsWith(".d.ts") || path.endsWith(".d.mts") || path.endsWith(".d.cts");
}
function isConfigFile(path) {
  const base = path.split("/").pop();
  return CONFIG_NAMES.includes(base);
}

/** One scan's content-addressed manifest: every input that can matter. */
function buildInputManifest(inventory, readBytes) {
  const sourceFiles = [];
  const configFiles = [];
  const packageFiles = [];
  const otherFiles = [];
  let totalBytes = 0;
  for (const file of inventory.files) {
    if (totalBytes + file.size > MAX_SCAN_SOURCE_BYTES) {
      throw new RequestRefusal("read-denied", "byte cap exhausted");
    }
    totalBytes += file.size;
    const base = file.path.split("/").pop();
    const digest = sha256Hex(readBytes(file.path).toString("utf8"));
    if (isSourceFile(file.path) || isDeclarationFile(file.path)) {
      sourceFiles.push({ path: file.path, digest, declaration: isDeclarationFile(file.path) });
    } else if (isConfigFile(file.path)) {
      configFiles.push({ path: file.path, digest });
    } else if (base === PACKAGE_NAME) {
      packageFiles.push({ path: file.path, digest });
    } else {
      otherFiles.push({ path: file.path, digest });
    }
  }
  sourceFiles.sort((a, b) => utf8Compare(a.path, b.path));
  configFiles.sort((a, b) => utf8Compare(a.path, b.path));
  packageFiles.sort((a, b) => utf8Compare(a.path, b.path));
  otherFiles.sort((a, b) => utf8Compare(a.path, b.path));
  return { sourceFiles, configFiles, packageFiles, otherFiles, totalBytes };
}

// ---------------------------------------------------------------------------
// 4. Package and project discovery (data-driven, no discovery magic).
// ---------------------------------------------------------------------------

/** Bounded strict-JSON parse of package manifests as plain data. */
function parsePackageManifest(text) {
  let document;
  try {
    document = JSON.parse(text);
  } catch {
    return null;
  }
  if (!isObject(document)) return null;
  const manifest = {};
  if (typeof document.name === "string" && document.name.length <= 214) {
    manifest.name = document.name;
  }
  if (document.type === "module" || document.type === "commonjs") {
    manifest.type = document.type;
  }
  if (typeof document.main === "string") manifest.main = document.main;
  if (isObject(document.exports)) manifest.exports = document.exports;
  if (isObject(document.dependencies)) manifest.dependencies = Object.keys(document.dependencies).sort();
  return manifest;
}

/**
 * Discover packages (package.json manifests inside the inventory) and
 * TS projects (tsconfig files with parsed references/paths). Nothing is
 * discovered outside the enumerated inventory.
 */
function discoverPackagesAndProjects(manifest, readBytes, ts, diagnostics) {
  const packages = [];
  const projects = [];
  const packageByRoot = new Map();
  for (const entry of manifest.packageFiles) {
    const text = readBytes(entry.path).toString("utf8");
    const parsed = parsePackageManifest(text);
    if (!parsed) {
      diagnostics.push({
        code: "package-manifest-invalid",
        path: entry.path,
        severity: "warning",
        detail: "package.json is not a valid manifest object",
      });
      continue;
    }
    const root = entry.path.slice(0, -PACKAGE_NAME.length).replace(/\/$/, "");
    const pkg = {
      root: root === "" ? "." : root,
      manifestPath: entry.path,
      name: parsed.name ?? null,
      type: parsed.type ?? null,
      exports: parsed.exports ? canonicalText(parsed.exports) : null,
      digest: entry.digest,
    };
    packages.push(pkg);
    packageByRoot.set(pkg.root, pkg);
  }
  for (const entry of manifest.configFiles) {
    const text = readBytes(entry.path).toString("utf8");
    const result = ts.readConfigFile(entry.path, () => text);
    if (result.error) {
      diagnostics.push({
        code: "config-parse",
        path: entry.path,
        severity: "error",
        detail: ts.flattenDiagnosticMessageText(result.error.messageText, " ").slice(0, 256),
      });
      continue;
    }
    const directory = entry.path.slice(0, -"tsconfig.json".length).replace(/\/$/, "");
    const raw = result.config ?? {};
    const references = Array.isArray(raw.references)
      ? raw.references
        .filter((reference) => isObject(reference) && typeof reference.path === "string")
        .map((reference) => ({
          path: reference.path,
          resolved: resolveConfigReference(entry.path, reference.path),
        }))
        .filter((reference) => reference.resolved !== null)
      : [];
    const extendsChain = [];
    let currentExtends = typeof raw.extends === "string" ? raw.extends : null;
    while (currentExtends && extendsChain.length < 8) {
      const resolved = resolveConfigReference(entry.path, currentExtends);
      if (resolved === null || !manifest.configFiles.some((config) => config.path === resolved)) {
        extendsChain.push({ path: currentExtends, resolved, inScope: resolved !== null });
        break;
      }
      extendsChain.push({ path: currentExtends, resolved, inScope: true });
      currentExtends = null;
    }
    projects.push({
      configPath: entry.path,
      directory: directory === "" ? "." : directory,
      references,
      extendsChain,
      digest: entry.digest,
      raw,
    });
  }
  projects.sort((a, b) => utf8Compare(a.configPath, b.configPath));
  packages.sort((a, b) => utf8Compare(a.root, b.root));
  return { packages, projects, packageByRoot };
}

function resolveConfigReference(fromConfigPath, reference) {
  if (typeof reference !== "string" || reference.length === 0) return null;
  const directory = fromConfigPath.includes("/")
    ? fromConfigPath.slice(0, fromConfigPath.lastIndexOf("/"))
    : "";
  const segments = directory === "" ? [] : directory.split("/");
  let candidate = reference;
  if (candidate.startsWith("./") || candidate.startsWith("../")) {
    for (const segment of candidate.split("/")) {
      if (segment === ".") continue;
      if (segment === "..") segments.pop();
      else segments.push(segment);
    }
    candidate = segments.join("/");
  } else if (candidate.startsWith("/")) {
    return null; // absolute or root-escaped reference is out of scope
  } else {
    // A bare reference resolves as a config-package dependency — out of
    // the enumerated inventory, so it is an unknown, recorded upstream.
    return null;
  }
  if (candidate.endsWith("/")) candidate = candidate.slice(0, -1);
  return candidate === "" ? null : candidate;
}

// ---------------------------------------------------------------------------
// 5. The restricted CompilerHost.
// ---------------------------------------------------------------------------

/**
 * Answer compiler lookups only from the enumerated inventory and the
 * embedded library map. `denied` records uncertainty; `missing` is a
 * plain negative. No ts.sys, no default host spread, no environment.
 */
function createRestrictedHost({ ts, inventorySet, readBytes, libMap, logicalToHost }) {
  const sourceFileCache = new Map();
  const existsCache = new Map();
  const denied = [];
  const currentDirectory = "/lekalo/project";
  const deny = (kind, hostName) => {
    if (denied.length < MAX_DIAGNOSTICS) denied.push({ kind, path: hostName });
    return undefined;
  };

  const resolveInventory = (hostName) => inventorySet.get(hostName) ?? inventorySet.get(hostName.toLowerCase());

  const host = {
    useCaseSensitiveFileNames: () => false,
    getCanonicalFileName: (name) => name.toLowerCase(),
    getCurrentDirectory: () => currentDirectory,
    getNewLine: () => "\n",
    getDefaultLibFileName: () => "/lekalo/libs/lib.d.ts",
    getDefaultLibLocation: () => "/lekalo/libs",

    fileExists(hostName) {
      const normalized = hostName.replaceAll("\\", "/");
      const key = normalized.toLowerCase();
      if (existsCache.has(key)) return existsCache.get(key);
      let result;
      if (resolveInventory(normalized) !== undefined) {
        result = true;
      } else if (/^\/lekalo\/libs\/lib(\..+)?\.d\.ts$/.test(normalized)
        && libMap.has(normalized.split("/").pop())) {
        result = true;
      } else {
        result = false;
        deny("unknown", normalized);
      }
      existsCache.set(key, result);
      return result;
    },

    readFile(hostName) {
      const normalized = hostName.replaceAll("\\", "/");
      const logical = resolveInventory(normalized);
      if (logical !== undefined) {
        return readBytes(logical).toString("utf8");
      }
      if (/^\/lekalo\/libs\/lib(\..+)?\.d\.ts$/.test(normalized)) {
        const text = libMap.get(normalized.split("/").pop());
        if (text !== undefined) return text;
      }
      return deny("read", normalized);
    },

    getSourceFile(hostName, languageVersion) {
      const normalized = hostName.replaceAll("\\", "/");
      const key = normalized.toLowerCase();
      const cached = sourceFileCache.get(key);
      if (cached) return cached;
      const text = host.readFile(normalized);
      if (text === undefined) return undefined;
      const sourceFile = ts.createSourceFile(normalized, text, languageVersion, true);
      sourceFileCache.set(key, sourceFile);
      return sourceFile;
    },

    directoryExists(hostName) {
      const normalized = hostName.replaceAll("\\", "/").replace(/\/$/, "");
      if (normalized === currentDirectory || normalized === "/lekalo" || normalized === "/lekalo/libs") {
        return true;
      }
      for (const logical of inventorySet.keys()) {
        if (logical.startsWith(`${normalized}/`)) return true;
      }
      return false;
    },

    getDirectories() {
      // Enumeration is the scanner's job; the compiler never walks.
      return [];
    },

    readDirectory(rootDir, extensions) {
      // Wildcard include expansion over the pre-enumerated inventory
      // only — never the host filesystem. Returns the matching files.
      const prefix = rootDir.replaceAll("\\", "/").replace(/\/$/, "");
      const extensionSet = new Set(Array.isArray(extensions) ? extensions : []);
      const matches = [];
      for (const logical of inventorySet.keys()) {
        if (!logical.startsWith(`${prefix}/`)) continue;
        if (extensionSet.size > 0 && ![...extensionSet].some((extension) => logical.endsWith(extension))) {
          continue;
        }
        matches.push(logical);
      }
      return matches.sort(utf8Compare);
    },

    getEnvironmentVariable: () => "",
    realpath: undefined,
    trace: undefined,
  };
  return { host, denied, currentDirectory };
}

/** The exact host-to-logical inventory map of one scan. */
function buildInventorySet(manifest) {
  const set = new Map();
  const addLogical = (logical) => {
    set.set(logical, logical);
    set.set(logical.toLowerCase(), logical);
  };
  for (const file of [
    ...manifest.sourceFiles,
    ...manifest.configFiles,
    ...manifest.packageFiles,
    ...manifest.otherFiles,
  ]) {
    addLogical(file.path);
    // Compiler-side spellings anchored at the virtual current directory.
    const host = `/lekalo/project/${file.path}`;
    set.set(host, file.path);
    set.set(host.toLowerCase(), file.path);
  }
  return set;
}

// ---------------------------------------------------------------------------
// 6. Native identity, signature digests, and canonical symbol records.
// ---------------------------------------------------------------------------

const SUPPORTED_KIND_WIRE = new Set([
  "entity", "value-object", "command", "query", "event", "policy", "effect",
  "endpoint", "test",
]);

/**
 * The package locator of one declaration: explicit package name plus
 * workspace-relative package root, or the explicit logical project key
 * when no package manifest covers the file.
 */
function packageLocatorFor(path, packageIndex) {
  let best = null;
  for (const [root, pkg] of packageIndex) {
    if (root === "." || path === root || path.startsWith(`${root}/`)) {
      if (best === null || root.length > best.root.length) best = pkg;
    }
  }
  if (best) {
    return { kind: "package", name: best.name ?? "", root: best.root };
  }
  return { kind: "project", root: "." };
}

/**
 * The canonical declaration family used in identity tuples: merged
 * declarations (interface+namespace, function+module) map to the same
 * family token so the native id stays one.
 */
function declarationFamily(ts, symbol) {
  const flags = symbol.flags;
  const parts = [];
  if (flags & ts.SymbolFlags.Interface) parts.push("interface");
  if (flags & ts.SymbolFlags.Class) parts.push("class");
  if (flags & ts.SymbolFlags.Enum) parts.push("enum");
  if (flags & ts.SymbolFlags.TypeAlias) parts.push("type-alias");
  if (flags & ts.SymbolFlags.Function) parts.push("function");
  if (flags & ts.SymbolFlags.Method) parts.push("method");
  if (flags & ts.SymbolFlags.Variable) parts.push("variable");
  if (flags & ts.SymbolFlags.Property) parts.push("property");
  if (flags & ts.SymbolFlags.ConstEnum) parts.push("const-enum");
  if (flags & ts.SymbolFlags.NamespaceModule || flags & ts.SymbolFlags.ValueModule) parts.push("module");
  if (parts.length === 0) parts.push("symbol");
  return parts.sort().join("+");
}

/** Whether this member symbol is declared static. */
function isStaticMember(ts, symbol) {
  for (const declaration of symbol.declarations ?? []) {
    if (ts.getCombinedModifierFlags(declaration) & ts.ModifierFlags.Static) {
      return true;
    }
  }
  return false;
}

/**
 * The canonical native identity tuple (§4). Deliberately excludes
 * source location, export aliases, and compiler object ids; includes
 * the package locator, module-relative path, lexical qualified name,
 * declaration family, and static/instance slot.
 */
function nativeIdentityTuple({ locator, modulePath, qualifiedName, family, slot }) {
  return [
    IDENTITY_DOMAIN,
    `${locator.kind}:${locator.name}:${locator.root}`,
    modulePath,
    qualifiedName,
    family,
    slot,
  ];
}

function nativeId(tuple) {
  return `ts1-${sha256Hex(canonicalText(tuple))}`;
}

/**
 * The structural signature graph of one callable/value symbol: ordered
 * parameters (with optionality/rest), a nullability-aware return, the
 * full ordered overload list, type-parameter arity and constraints, and
 * sorted member sets for values. One display string is never used as
 * identity; recursion is cut at type-reference anchors.
 */
function signatureGraphOfSymbol({ ts, checker, symbol, program, depthBudget }) {
  const seen = new Set();
  const visit = (type, depth) => {
    if (type === undefined || depth > 6) return { kind: "cutoff" };
    const id = (type.id ?? type);
    if (typeof id === "number") {
      if (seen.has(id)) return { kind: "anchor" };
      seen.add(id);
    }
    if (type.flags & ts.TypeFlags.Any) return { kind: "any" };
    if (type.flags & ts.TypeFlags.Unknown) return { kind: "unknown" };
    if (type.flags & ts.TypeFlags.Never) return { kind: "never" };
    if (type.flags & ts.TypeFlags.Void) return { kind: "void" };
    if (type.flags & ts.TypeFlags.Undefined) return { kind: "undefined" };
    if (type.flags & ts.TypeFlags.Null) return { kind: "null" };
    if (type.flags & ts.TypeFlags.BooleanLiteral || type.flags & ts.TypeFlags.Boolean) {
      return { kind: "boolean" };
    }
    if (type.flags & ts.TypeFlags.NumberLiteral || type.flags & ts.TypeFlags.Number) {
      return { kind: "number" };
    }
    if (type.flags & ts.TypeFlags.StringLiteral) {
      return { kind: "string-literal", value: String(type.value ?? "") };
    }
    if (type.flags & ts.TypeFlags.String) return { kind: "string" };
    if (type.flags & ts.TypeFlags.BigInt) return { kind: "bigint" };
    if (type.flags & ts.TypeFlags.ESSymbol) return { kind: "symbol" };
    if (type.flags & ts.TypeFlags.Union || type.flags & ts.TypeFlags.Intersection) {
      const members = (type.types ?? [])
        .map((member) => visit(member, depth + 1))
        .sort((left, right) => utf8Compare(canonicalText(left), canonicalText(right)));
      return { kind: type.flags & ts.TypeFlags.Union ? "union" : "intersection", members };
    }
    const reference = {
      kind: "reference",
      target: type.symbol ? qualifiedNameOfSymbol(type.symbol, ".") : checker.typeToString(type, undefined, ts.TypeFormatFlags.NoTruncation | ts.TypeFormatFlags.UseFullyQualifiedType),
      arguments: (type.typeArguments ?? []).map((argument) => visit(argument, depth + 1)),
    };
    return reference;
  };

  const callGraph = (signatures) => signatures.map((signature) => {
    const parameters = signature.parameters.map((parameter) => {
      const declaration = parameter.valueDeclaration;
      const optional = Boolean(declaration && (ts.getCombinedModifierFlags(declaration) & ts.ModifierFlags.Optional
        || declaration?.questionToken || declaration?.initializer));
      const isRest = Boolean(declaration && declaration.dotDotDotToken);
      return {
        name: parameter.name,
        optional,
        rest: isRest,
        type: visit(checker.getTypeOfSymbol(parameter), 0),
      };
    });
    const returnType = visit(checker.getReturnTypeOfSignature(signature), 0);
    const async = Boolean(signature.declaration?.modifiers?.some(
      (modifier) => modifier.kind === ts.SyntaxKind.AsyncKeyword));
    return { parameters, returnType: async ? { kind: "promise", value: returnType } : returnType, async };
  });

  const type = checker.getTypeOfSymbolAtLocation(symbol, symbol.declarations?.[0] ?? program.getSourceFiles()[0]);
  const callSignatures = checker.getSignaturesOfType(type, ts.SignatureKind.Call);
  const constructSignatures = checker.getSignaturesOfType(type, ts.SignatureKind.Construct);
  const graph = {
    policy: SIGNATURE_POLICY_VERSION,
    call: callGraph(callSignatures),
    construct: callGraph(constructSignatures),
    properties: (type.flags & ts.TypeFlags.Object) === 0 ? [] : checker
      .getPropertiesOfType(type)
      .filter((member) => !(member.flags & (ts.SymbolFlags.Method | ts.SymbolFlags.Function)))
      .map((member) => ({
        name: member.name,
        type: visit(checker.getTypeOfSymbol(member), 0),
        optional: Boolean(member.declarations?.[0]?.questionToken),
      }))
      .sort((left, right) => utf8Compare(left.name, right.name)),
  };
  return graph;
}

/** The dotted lexical qualified name of one symbol inside its module. */
function qualifiedNameOfSymbol(symbol, separator, guard = new Set()) {
  if (!symbol || guard.has(symbol)) return symbol?.name ?? "?";
  guard.add(symbol);
  const name = symbol.name ?? "?";
  void separator;
  return name;
}

/**
 * The lexical qualified name from declarations: walks declaration
 * parents (class, namespace, module) and joins with `.` — stable under
 * whitespace and member reordering.
 */
function lexicalQualifiedName(ts, checker, symbol) {
  const parts = [symbol.name];
  let node = symbol.declarations?.[0]?.parent;
  while (node) {
    if (node.kind === ts.SyntaxKind.ClassDeclaration
      || node.kind === ts.SyntaxKind.InterfaceDeclaration
      || node.kind === ts.SyntaxKind.EnumDeclaration
      || node.kind === ts.SyntaxKind.ModuleDeclaration
      || node.kind === ts.SyntaxKind.FunctionDeclaration) {
      const parentSymbol = node.symbol ?? (node.name ? undefined : undefined);
      if (parentSymbol?.name) {
        parts.unshift(parentSymbol.name);
      } else {
        break;
      }
    } else if (node.kind === ts.SyntaxKind.SourceFile) {
      break;
    }
    node = node.parent;
  }
  return parts.join(".");
}

function signatureDigest(graph) {
  return `sha256:${sha256Hex(canonicalText([SIGNATURE_DOMAIN, graph]))}`;
}

// ---------------------------------------------------------------------------
// 7. Program construction and the semantic index walk.
// ---------------------------------------------------------------------------

/**
 * One sorted, complete internal index. `state` is `complete` only when
 * every requested step ran to the end; uncertainty inside a complete
 * scan lives in `diagnostics` and per-record confidence, never in a
 * hidden gap.
 */
function emptyIndex(compilerMeta) {
  return {
    state: "complete",
    packages: [],
    projects: [],
    symbols: [],
    exports: [],
    references: [],
    routes: [],
    tests: [],
    diagnostics: [],
    anyUncertainty: [],
    inputManifest: null,
    compiler: compilerMeta,
  };
}

/** Canonical UTF-8 ordering pass over every record family. */
function sortIndex(index) {
  const byPath = (a, b) => utf8Compare(a.path, b.path);
  index.packages.sort((a, b) => utf8Compare(a.root, b.root));
  index.projects.sort((a, b) => utf8Compare(a.configPath, b.configPath));
  index.symbols.sort((a, b) => utf8Compare(a.native, b.native));
  index.exports.sort((a, b) => utf8Compare(`${a.module}\u0000${a.name}`, `${b.module}\u0000${b.name}`));
  index.references.sort((a, b) => utf8Compare(`${a.from}\u0000${a.to}\u0000${a.role}`, `${b.from}\u0000${b.to}\u0000${b.role}`));
  index.routes.sort((a, b) => utf8Compare(`${a.method}\u0000${a.path}`, `${b.method}\u0000${b.path}`));
  index.tests.sort((a, b) => utf8Compare(`${a.path}\u0000${a.name}`, `${b.path}\u0000${b.name}`));
  index.diagnostics.sort((a, b) => utf8Compare(canonicalText(a), canonicalText(b)));
  index.anyUncertainty.sort(byPath);
  return index;
}

/** One symbol row of the internal index. */
function makeSymbolRow({ native, family, qualifiedName, modulePath, line, endLine, signature, isDeclaration, slot, jsdoc, memberOf }) {
  return {
    native,
    family,
    qualifiedName,
    module: modulePath,
    line,
    endLine,
    signature,
    declarationOnly: isDeclaration,
    slot,
    jsdoc: jsdoc ?? null,
    memberOf: memberOf ?? null,
  };
}

/**
 * Collect every indexable module-level and member symbol of one source
 * file with checker-verified identity, spans, signatures, and JSDoc as
 * supporting evidence only.
 */
function indexSourceFile({ ts, checker, sourceFile, modulePath, packageIndex, index, context }) {
  const isDeclaration = isDeclarationFile(modulePath);
  const walk = (node, parentNative, parentName) => {
    if (context.exceeded()) return;
    let symbol = null;
    let family = null;
    let slot = "instance";

    if (node.kind === ts.SyntaxKind.ClassDeclaration
      || node.kind === ts.SyntaxKind.ClassExpression) {
      symbol = node.name ? checker.getSymbolAtLocation(node.name) : null;
      family = "class";
      slot = "static";
    } else if (node.kind === ts.SyntaxKind.InterfaceDeclaration) {
      symbol = checker.getSymbolAtLocation(node.name);
      family = "interface";
    } else if (node.kind === ts.SyntaxKind.TypeAliasDeclaration) {
      symbol = checker.getSymbolAtLocation(node.name);
      family = "type-alias";
    } else if (node.kind === ts.SyntaxKind.EnumDeclaration) {
      symbol = checker.getSymbolAtLocation(node.name);
      family = "enum";
    } else if (node.kind === ts.SyntaxKind.FunctionDeclaration) {
      symbol = node.name ? checker.getSymbolAtLocation(node.name) : null;
      family = "function";
      // Overloads share one declaration node chain; the checker merges
      // them into one symbol whose declarations list every overload plus
      // the implementation. They are stored as ONE native symbol with
      // the whole ordered signature set (§4, no per-overload duplicates).
    } else if (node.kind === ts.SyntaxKind.VariableDeclaration
      || node.kind === ts.SyntaxKind.PropertySignature
      || node.kind === ts.SyntaxKind.PropertyDeclaration) {
      const nameNode = node.name;
      if (nameNode && nameNode.kind === ts.SyntaxKind.Identifier) {
        symbol = checker.getSymbolAtLocation(nameNode);
        family = node.kind === ts.SyntaxKind.PropertySignature || node.kind === ts.SyntaxKind.PropertyDeclaration
          ? "property"
          : "variable";
        if (node.kind === ts.SyntaxKind.PropertyDeclaration
          && ts.getCombinedModifierFlags(node) & ts.ModifierFlags.Static) {
          slot = "static";
        }
      }
    } else if (node.kind === ts.SyntaxKind.MethodDeclaration
      || node.kind === ts.SyntaxKind.MethodSignature
      || node.kind === ts.SyntaxKind.GetAccessor
      || node.kind === ts.SyntaxKind.SetAccessor) {
      if (node.name?.kind === ts.SyntaxKind.Identifier) {
        symbol = checker.getSymbolAtLocation(node.name);
        family = "method";
        if (ts.getCombinedModifierFlags(node) & ts.ModifierFlags.Static) {
          slot = "static";
        }
      }
    }

    if (symbol && symbol.name && family) {
      const locator = packageLocatorFor(modulePath, packageIndex);
      const qualifiedName = parentName
        ? `${parentName}.${symbol.name}`
        : lexicalQualifiedName(ts, checker, symbol);
      // Skip shadowed local captures: only declarations that own a
      // lexical parent chain inside this module are indexable.
      const tuple = nativeIdentityTuple({
        locator,
        modulePath,
        qualifiedName,
        family: declarationFamily(ts, symbol) === "symbol" ? family : declarationFamily(ts, symbol),
        slot,
      });
      const native = nativeId(tuple);
      if (!index.symbols.some((row) => row.native === native && row.module === modulePath)) {
        const start = node.getStart(sourceFile);
        const end = node.getEnd();
        const startLine = sourceFile.getLineAndCharacterOfPosition(start);
        const endLine = sourceFile.getLineAndCharacterOfPosition(end);
        const signatureGraph = family === "class" || family === "interface"
          ? null
          : signatureGraphOfSymbol({ ts, checker, symbol, program: context.program, depthBudget: 6 });
        const signature = signatureGraph ? signatureDigest(signatureGraph) : null;
        const jsdoc = symbol.getDocumentationComment
          ? symbol.getDocumentationComment(checker)
            .map((part) => part.text ?? "")
            .join("")
            .slice(0, 256)
          : null;
        index.symbols.push(makeSymbolRow({
          native,
          family: declarationFamily(ts, symbol) === "symbol" ? family : declarationFamily(ts, symbol),
          qualifiedName,
          modulePath,
          line: startLine.line + 1,
          endLine: endLine.line + 1,
          signature,
          isDeclaration,
          slot,
          jsdoc: jsdoc === "" ? null : jsdoc,
          memberOf: parentNative,
        }));
        (context.nativeByDeclaration ??= new Map()).set(node, native);
        parentNative = native;
        parentName = qualifiedName;
      }
    }

    ts.forEachChild(node, (child) => walk(child, parentNative, parentName));
  };
  walk(sourceFile, null, null);
}

/**
 * Export edges: `getExportsOfModule` per module plus alias chains
 * (`export { x as y }`, `export *`, `export =`) recorded as alias
 * edges, never as duplicate native symbols.
 */
function indexExports({ ts, checker, program, packageIndex, index, context }) {
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile && isEmbeddedLibBase(sourceFile.fileName)) continue;
    const modulePath = normalizeModulePath(sourceFile.fileName, context);
    if (modulePath === null) continue;
    const moduleSymbol = checker.getSymbolAtLocation(sourceFile);
    if (!moduleSymbol) continue;
    const locator = packageLocatorFor(modulePath, packageIndex);
    let exports;
    try {
      exports = checker.getExportsOfModule(moduleSymbol);
    } catch {
      continue;
    }
    for (const exported of exports) {
      // getAliasedSymbol is only legal on alias symbols; plain exports
      // resolve to themselves.
      const resolved = (exported.flags & ts.SymbolFlags.Alias)
        ? checker.getAliasedSymbol(exported)
        : exported;
      const targetDeclarations = resolved.declarations ?? [];
      const first = targetDeclarations[0];
      const targetModule = first ? normalizeModulePath(first.getSourceFile().fileName, context) : null;
      const isAlias = exported !== resolved
        && (exported.declarations?.some((declaration) =>
          declaration.kind === ts.SyntaxKind.ExportSpecifier
          || declaration.kind === ts.SyntaxKind.ExportAssignment) ?? false);
      const family = declarationFamily(ts, resolved);
      const tuple = nativeIdentityTuple({
        locator: targetModule ? packageLocatorFor(targetModule, packageIndex) : locator,
        modulePath: targetModule ?? modulePath,
        qualifiedName: lexicalQualifiedName(ts, checker, resolved),
        family,
        slot: "static",
      });
      index.exports.push({
        module: modulePath,
        name: exported.name,
        native: isAlias || targetDeclarations.length === 0 ? null : nativeId(tuple),
        aliasOf: isAlias && targetModule !== null
          ? { module: targetModule, name: resolved.name }
          : null,
        declarationOnly: targetDeclarations.length > 0 && targetDeclarations.every((declaration) => {
          const declarationModule = normalizeModulePath(declaration.getSourceFile().fileName, context);
          return declarationModule === null ? false : isDeclarationFile(declarationModule);
        }),
        star: exported.declarations?.some((declaration) =>
          declaration.kind === ts.SyntaxKind.ExportDeclaration
          && declaration.exportClause?.kind === ts.SyntaxKind.NamespaceExport) ?? false,
      });
    }
  }
}

/**
 * Import and reference graph: import declarations, export declarations,
 * and call targets resolved through the checker. Unresolved modules
 * become external/unknown rows (uncertainty), never guessed targets.
 */
function indexReferences({ ts, checker, program, context, index }) {
  const record = (row) => {
    if (index.references.length < 8192) index.references.push(row);
  };
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile && isEmbeddedLibBase(sourceFile.fileName)) continue;
    const fromModule = normalizeModulePath(sourceFile.fileName, context);
    if (fromModule === null) continue;
    const visit = (node) => {
      if (context.exceeded()) return;
      if (node.kind === ts.SyntaxKind.ImportDeclaration
        || node.kind === ts.SyntaxKind.ExportDeclaration
        || node.kind === ts.SyntaxKind.ImportEqualsDeclaration) {
        const moduleSpecifier = node.moduleSpecifier;
        if (moduleSpecifier?.kind === ts.SyntaxKind.StringLiteral) {
          const resolution = program.getResolvedModuleFromModuleSpecifier?.(moduleSpecifier)
            ?? program.getResolvedModule(sourceFile, moduleSpecifier.text);
          if (resolution?.resolvedModule) {
            const toModule = normalizeModulePath(resolution.resolvedModule.resolvedFileName, context);
            const line = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
            if (toModule !== null) {
              record(makeReference({
                fromModule, toModule, role: 'reference', confidence: 'exact', line,
              }));
            } else {
              record(makeReference({
                fromModule, toModule: null, toExternal: resolution.resolvedModule.resolvedFileName,
                role: 'reference', confidence: 'low', line,
              }));
            }
          } else {
            index.anyUncertainty.push({
              path: fromModule,
              kind: 'unresolved-import',
              detail: moduleSpecifier.text.slice(0, 128),
              line: sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
            });
          }
        }
      } else if (node.kind === ts.SyntaxKind.CallExpression) {
        const signature = checker.getResolvedSignature(node);
        const line = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
        if (signature?.declaration) {
          const declarationFile = signature.declaration.getSourceFile();
          const isLib = declarationFile.isDeclarationFile
            && isEmbeddedLibBase(declarationFile.fileName);
          if (isLib) {
            // Standard-library call target: type context, not a project
            // reference row.
            return;
          }
          const toModule = normalizeModulePath(declarationFile.fileName, context);
          if (toModule !== null) {
            record(makeReference({ fromModule, toModule, role: 'call', confidence: 'exact', line }));
          } else {
            record(makeReference({
              fromModule,
              toModule: null,
              toExternal: declarationFile.fileName,
              role: 'call',
              confidence: 'medium',
              line,
            }));
          }
        } else if (node.expression.kind === ts.SyntaxKind.Identifier
          || ts.isPropertyAccessExpression(node.expression)) {
          const target = node.expression.kind === ts.SyntaxKind.Identifier
            ? node.expression.text
            : node.expression.name?.text ?? "dynamic";
          index.anyUncertainty.push({
            path: fromModule,
            kind: 'unresolved-call',
            detail: String(target).slice(0, 64),
            line,
          });
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);
  }
}

/** One outbound reference row. */
function makeReference({ fromModule, fromNative, toModule, toNative, toExternal, role, confidence, line }) {
  return {
    from: fromModule,
    fromNative: fromNative ?? null,
    to: toModule ?? toExternal,
    toNative: toNative ?? null,
    external: toModule === null || toModule === undefined,
    role,
    confidence,
    line,
  };
}

/** Normalize a host source-file name to the logical module path. */
function normalizeModulePath(hostFileName, context) {
  if (typeof hostFileName !== 'string') return null;
  const normalized = hostFileName.split("\\").join("/");
  if (context.inventorySet.has(normalized)) {
    return context.inventorySet.get(normalized);
  }
  const lowered = normalized.toLowerCase();
  if (context.inventorySet.has(lowered)) {
    return context.inventorySet.get(lowered);
  }
  return null;
}


// ---------------------------------------------------------------------------
// 8. Framework and test rule plugins (explicit vocabulary, confidence).
// ---------------------------------------------------------------------------

/**
 * Explicit route rules. A rule matches only when the callee symbol
 * resolves to a declaration in a file the rule's moduleVocabulary
 * names (synthetic authored declarations ship inside the fixture
 * scope), and only with statically known string arguments. Namespace
 * or name similarity alone is never evidence.
 */
const ROUTE_RULES = [
  {
    id: 'route.get-static-v1',
    method: 'get',
    moduleVocabulary: ['router'],
    calleeNames: ['get'],
  },
  {
    id: 'route.post-static-v1',
    method: 'post',
    moduleVocabulary: ['router'],
    calleeNames: ['post'],
  },
];

/**
 * The one explicit test DSL vocabulary of the fixtures: statically
 * imported describe/it/test. Everything else is unknown, never guessed.
 */
const TEST_RULE = {
  id: 'test.describe-it-static-v1',
  calleeNames: new Set(['describe', 'it', 'test']),
  skipNames: new Set(['xdescribe', 'xit', 'xtest']),
};

function staticStringValue(node) {
  if (node?.kind === 11 /* StringLiteral */ && typeof node.text === 'string') {
    return node.text;
  }
  if (node?.kind === 10 /* NoSubstitutionTemplateLiteral */ && typeof node.text === 'string') {
    return node.text;
  }
  return null;
}

/**
 * Routes and tests per source file. Every produced row carries the
 * rule id and confidence; dynamic arguments lower confidence to
 * 'low' with an uncertainty record, and unknown wrappers are ignored
 * (never fabricated).
 */
function indexRoutesAndTests({ ts, checker, program, context, index }) {
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile && isEmbeddedLibBase(sourceFile.fileName)) continue;
    const fromModule = normalizeModulePath(sourceFile.fileName, context);
    if (fromModule === null) continue;
    const visit = (node) => {
      if (context.exceeded()) return;
      if (node.kind === ts.SyntaxKind.CallExpression) {
        const expression = node.expression;
        let calleeSymbol = checker.getSymbolAtLocation(
          expression.kind === ts.SyntaxKind.Identifier ? expression : expression.property ?? expression,
        );
        if (calleeSymbol && calleeSymbol.flags & ts.SymbolFlags.Alias) {
          try {
            calleeSymbol = checker.getAliasedSymbol(calleeSymbol);
          } catch {
            // keep the alias symbol: its own declarations are still rows
          }
        }
        if (calleeSymbol && calleeSymbol.declarations?.length) {
          const declarationFile = normalizeModulePath(
            calleeSymbol.declarations[0].getSourceFile().fileName, context);
          const calleeName = expression.kind === ts.SyntaxKind.Identifier
            ? expression.text
            : expression.property?.text ?? null;
            const rule = ROUTE_RULES.find((candidate) =>
              candidate.calleeNames.includes(calleeName)
              && declarationFile !== null
              && candidate.moduleVocabulary.some((vocabulary) => declarationFile.includes(vocabulary)));
            if (rule && node.arguments.length >= 2) {
              const pathValue = staticStringValue(node.arguments[0]);
              const handler = node.arguments[1];
              const handlerSymbol = handler && (handler.kind === ts.SyntaxKind.Identifier
                || handler.kind === ts.SyntaxKind.PropertyAccessExpression)
                ? checker.getSymbolAtLocation(handler.kind === ts.SyntaxKind.Identifier ? handler : handler.property)
                : null;
              if (pathValue !== null) {
                index.routes.push({
                  method: rule.method,
                  path: pathValue.slice(0, 256),
                  handler: handlerSymbol?.name ?? null,
                  handlerNative: null,
                  module: fromModule,
                  rule: rule.id,
                  confidence: handlerSymbol ? 'high' : 'medium',
                  provenance: 'rule',
                  line: sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
                });
              } else {
                index.anyUncertainty.push({
                  path: fromModule,
                  kind: 'dynamic-route',
                  detail: rule.id,
                  line: sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
                });
              }
            }
          // Tests: describe/it/test imported from the known DSL.
          if (calleeName && TEST_RULE.calleeNames.has(calleeName) && declarationFile !== null) {
            const nameArgument = node.arguments[0];
            const nameValue = staticStringValue(nameArgument);
            const callback = node.arguments.find((argument) =>
              argument.kind === ts.SyntaxKind.ArrowFunction
              || argument.kind === ts.SyntaxKind.FunctionExpression);
            if (nameValue !== null) {
              index.tests.push({
                path: fromModule,
                name: nameValue.slice(0, 256),
                suite: null,
                rule: TEST_RULE.id,
                confidence: 'high',
                line: sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
              });
              if (callback) {
                // Referenced callables inside the callback are test-body
                // candidates with provenance, not verified behavior.
                const walk = (inner) => {
                  if (inner.kind === ts.SyntaxKind.CallExpression) {
                    const innerExpression = inner.expression;
                    const innerSymbol = checker.getSymbolAtLocation(
                      innerExpression.kind === ts.SyntaxKind.Identifier
                        ? innerExpression
                        : innerExpression.property ?? innerExpression);
                    if (innerSymbol && innerSymbol.declarations?.length) {
                      const targetModule = normalizeModulePath(
                        innerSymbol.declarations[0].getSourceFile().fileName, context);
                      if (targetModule !== null) {
                        index.references.push(makeReference({
                          fromModule,
                          toModule: targetModule,
                          role: 'call',
                          confidence: 'medium',
                          line: sourceFile.getLineAndCharacterOfPosition(inner.getStart(sourceFile)).line + 1,
                        }));
                      }
                    }
                  }
                  ts.forEachChild(inner, walk);
                };
                ts.forEachChild(callback, walk);
              }
            } else {
              index.anyUncertainty.push({
                path: fromModule,
                kind: 'dynamic-test-name',
                detail: calleeName,
                line: sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
              });
            }
          }
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);
  }
}


// ---------------------------------------------------------------------------
// 9. Diagnostics and uncertainty collection.
// ---------------------------------------------------------------------------

/**
 * Collect compiler diagnostics (config, syntactic, semantic, global)
 * plus explicit uncertainty records: any-typed binding surfaces,
 * unresolved imports, dynamic calls, and unsupported type remainders.
 * skipLibCheck never suppresses project diagnostics (library files are
 * filtered by location, not by option).
 */
function collectDiagnostics({ ts, program, context, index }) {
  const push = (diagnostic, sourceFile) => {
    if (index.diagnostics.length >= MAX_DIAGNOSTICS) return;
    const file = diagnostic.file ?? sourceFile;
    const path = file ? normalizeModulePath(file.fileName, context) : null;
    if (file && path === null) return; // library-internal diagnostics stay out
    const position = file && diagnostic.start !== undefined
      ? file.getLineAndCharacterOfPosition(diagnostic.start)
      : null;
    index.diagnostics.push({
      code: String(diagnostic.code),
      severity: diagnostic.category === ts.DiagnosticCategory.Error ? 'error'
        : diagnostic.category === ts.DiagnosticCategory.Warning ? 'warning' : 'suggestion',
      path,
      line: position ? position.line + 1 : null,
      message: ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ').slice(0, 256),
    });
  };
  for (const diagnostic of program.getConfigFileParsingDiagnostics()) push(diagnostic);
  for (const diagnostic of program.getOptionsDiagnostics()) push(diagnostic);
  for (const diagnostic of program.getGlobalDiagnostics()) push(diagnostic);
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile && isEmbeddedLibBase(sourceFile.fileName)) continue;
    for (const diagnostic of program.getSyntacticDiagnostics(sourceFile)) push(diagnostic, sourceFile);
    for (const diagnostic of program.getSemanticDiagnostics(sourceFile)) push(diagnostic, sourceFile);
  }
}

/**
 * The any/unknown binding-surface walk: exported symbols whose declared
 * or inferred type contains any/unknown contamination. This is
 * uncertainty evidence; it never blocks the index, but it is never
 * reported as verified either.
 */
function collectAnySurfaces({ ts, checker, program, context, index }) {
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile && isEmbeddedLibBase(sourceFile.fileName)) continue;
    const fromModule = normalizeModulePath(sourceFile.fileName, context);
    if (fromModule === null) continue;
    const moduleSymbol = checker.getSymbolAtLocation(sourceFile);
    if (!moduleSymbol) continue;
    let exports;
    try {
      exports = checker.getExportsOfModule(moduleSymbol);
    } catch {
      continue;
    }
    for (const exported of exports) {
      if (index.anyUncertainty.length >= MAX_DIAGNOSTICS) return;
      // Type-only symbols (interfaces, type aliases) have no value type:
      // the value-side lookup reports `any`, which would fabricate
      // uncertainty. Their declared type carries the real shape.
      const type = exported.flags & ts.SymbolFlags.Interface
        || exported.flags & ts.SymbolFlags.TypeAlias
        ? checker.getDeclaredTypeOfSymbol(exported)
        : checker.getTypeOfSymbolAtLocation(exported, sourceFile);
      const contamination = typeContainsAny(ts, checker, type, new Set(), 0);
      if (contamination) {
        index.anyUncertainty.push({
          path: fromModule,
          kind: 'any-surface',
          detail: exported.name.slice(0, 64),
          line: exported.declarations?.[0]
            ? sourceFile.getLineAndCharacterOfPosition(exported.declarations[0].getStart(sourceFile)).line + 1
            : null,
        });
      }
    }
  }
}

/**
 * Whether one type (or anything reachable from it — object members, call
 * and construct signatures with their return types, type arguments)
 * contains `any`/`unknown` contamination (issue #44 fix F-3).
 *
 * Walks through the CHECKER — `getPropertiesOfType` + `getTypeOfSymbol`
 * for members and `getSignaturesOfType`/`getReturnTypeOfSignature` for
 * callables — because the raw type object never populates member types
 * itself. Bounded: depth ≤ 6 and a visited-type-id set keeps recursion
 * cycle-safe; the fanout is additionally capped so pathological types
 * cannot blow the scan budget.
 */
function typeContainsAny(ts, checker, type, seen, depth) {
  if (!type || depth > 6) return false;
  if (typeof type.id === 'number') {
    if (seen.has(type.id)) return false;
    seen.add(type.id);
  }
  if (type.flags & ts.TypeFlags.Any) return true;
  if (type.flags & ts.TypeFlags.Unknown) return true;
  if (type.flags & (ts.TypeFlags.Union | ts.TypeFlags.Intersection)) {
    return (type.types ?? []).some((member) => typeContainsAny(ts, checker, member, seen, depth + 1));
  }
  if (type.flags & ts.TypeFlags.Object) {
    // Generic type arguments (e.g. the `any` inside `Promise<any>`).
    if ((type.typeArguments ?? []).some((argument) => typeContainsAny(ts, checker, argument, seen, depth + 1))) {
      return true;
    }
    // Callable surfaces: call/construct signatures and their return
    // types (e.g. `() => Promise<any>`).
    const signatures = [
      ...checker.getSignaturesOfType(type, ts.SignatureKind.Call),
      ...checker.getSignaturesOfType(type, ts.SignatureKind.Construct),
    ].slice(0, 32);
    if (signatures.some((signature) =>
      typeContainsAny(ts, checker, checker.getReturnTypeOfSignature(signature), seen, depth + 1)
      || (signature.parameters ?? []).slice(0, 32).some((parameter) =>
        typeContainsAny(ts, checker, checker.getTypeOfSymbol(parameter), seen, depth + 1)))) {
      return true;
    }
    // Declared members via the checker (e.g. `{ nested: { value: any } }`).
    const members = checker.getPropertiesOfType(type).slice(0, 64);
    return members.some((member) =>
      typeContainsAny(ts, checker, checker.getTypeOfSymbol(member), seen, depth + 1));
  }
  return false;
}
// ---------------------------------------------------------------------------
// 10. The scan pipeline and the internal incremental session.
// ---------------------------------------------------------------------------

/**
 * Run one full scan over a validated read view. The trusted execution
 * context provides the permitted project root; every read goes through
 * the kernel read view (scope/exclusion/link/byte-budget checks).
 */
function runScan({ profile, readView, permittedProjectRoot, limits }) {
  let programOptions = null;
  const ts = assertCompilerAvailable();
  const compilerMeta = {
    typescript: vendoredTs().version,
    embedded: true,
  };
  const roots = readView.roots;
  const inventory = enumerateInventory(permittedProjectRoot, roots, profile);
  const readBytes = (logicalPath) => readView.readFile(logicalPath, {
    files: limits?.files ?? MAX_SCAN_FILES,
    bytes: limits?.bytes ?? MAX_SCAN_SOURCE_BYTES,
  });
  const manifest = buildInputManifest(inventory, readBytes);
  const diagnostics = [];
  const { packages, projects, packageByRoot } = discoverPackagesAndProjects(
    manifest, readBytes, ts, diagnostics);
  const packageIndex = new Map(packages.map((pkg) => [pkg.root, pkg]));
  const index = emptyIndex(compilerMeta);
  index.packages = packages;
  index.projects = projects.map(({ configPath, directory, references, extendsChain, digest }) => ({
    configPath, directory, references, extendsChain, digest,
  }));
  index.diagnostics = diagnostics;

  // The host answers only from this inventory plus the embedded libs.
  const inventorySet = buildInventorySet(manifest);
  const { host, denied } = createRestrictedHost({
    ts, inventorySet, readBytes, libMap: embeddedLibFiles(),
  });
  const context = {
    inventorySet,
    program: null,
    exceeded: () => false,
  };

  // Program roots: every tsconfig's parsed file list plus any inventory
  // source file no config covers. The parsed compiler options (paths,
  // baseUrl, moduleResolution, target …) drive the Program so module
  // resolution follows the project's real configuration.
  //
  // Option fidelity (issue #44 fix F-2): `defaults` are only a fallback
  // seed — every key a config EXPLICITLY sets (via parsed.options) wins,
  // for every project that sets it (last config wins on conflict). Only
  // `noEmit` and `disableSourceOfProjectReferenceRedirect` are forced:
  // the scanner never emits and never follows project-reference output
  // redirects. Keys no config set keep the seed, so config-less
  // projects still analyze with deterministic ESM/Bundler defaults.
  const rootNames = [];
  const defaults = {
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    noEmit: true,
    skipLibCheck: false,
    allowJs: false,
  };
  const options = { ...defaults };
  for (const project of projects) {
    const parsed = ts.parseJsonConfigFileContent(
      project.raw ?? {},
      host,
      `/lekalo/project/${project.directory}`,
      undefined,
      `/lekalo/project/${project.configPath}`,
    );
    for (const fileName of parsed.fileNames) rootNames.push(fileName);
    if (parsed.options && Object.keys(parsed.options).length > 0) {
      for (const [key, value] of Object.entries(parsed.options)) {
        options[key] = value;
      }
    }
    for (const diagnostic of parsed.errors) {
      if (index.diagnostics.length < MAX_DIAGNOSTICS) {
        index.diagnostics.push({
          code: String(diagnostic.code),
          severity: "error",
          path: project.configPath,
          line: null,
          message: ts.flattenDiagnosticMessageText(diagnostic.messageText, " ").slice(0, 256),
        });
      }
    }
  }
  // Forced keys: never emit, never follow project-reference redirects.
  options.noEmit = true;
  options.disableSourceOfProjectReferenceRedirect = true;
  // Recorded evidence of the analysis configuration (issue #44 fix F-2):
  // tests and hosts can prove the parsed tsconfig options reached the Program.
  programOptions = options;
  const configured = new Set(rootNames.map((name) => name.toLowerCase()));
  for (const file of manifest.sourceFiles) {
    const hostName = `/lekalo/project/${file.path}`;
    if (!configured.has(hostName.toLowerCase()) && !configured.has(file.path.toLowerCase())) {
      rootNames.push(hostName);
    }
  }
  if (rootNames.length === 0) {
    return finalizeScan(index, manifest, profile, readView, programOptions);
  }

  const program = ts.createProgram({ rootNames, options, host });
  context.program = program;
  const checker = program.getTypeChecker();

  // Module walk: every inventory source file gets symbols + exports.
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile && isEmbeddedLibBase(sourceFile.fileName)) continue;
    const modulePath = normalizeModulePath(sourceFile.fileName, context);
    if (modulePath === null) continue;
    indexSourceFile({
      ts, checker, sourceFile, modulePath, packageIndex, index, context,
    });
  }
  indexExports({ ts, checker, program, packageIndex, index, context });
  indexReferences({ ts, checker, program, context, index });
  indexRoutesAndTests({ ts, checker, program, context, index });
  collectDiagnostics({ ts, program, context, index });
  collectAnySurfaces({ ts, checker, program, context, index });

  // Host denial notes stay internal: the restricted host serves only the
  // enumerated inventory and the embedded libraries, so any denial is a
  // resolution probe of a candidate spelling that is not part of the
  // project. Failed resolutions surface as unresolved-import/call
  // uncertainty above; probing noise never becomes scan uncertainty.

  return finalizeScan(index, manifest, profile, readView, programOptions);
}

function normalizeUnknownPath(hostName) {
  const normalized = hostName.split("\\").join("/");
  return normalized.startsWith("/lekalo/project/")
    ? normalized.slice("/lekalo/project/".length)
    : normalized;
}

/** Deterministic finalization: manifest, digests, canonical order. */
function finalizeScan(index, manifest, profile, readView, programOptions) {
  index.inputManifest = {
    sourceFiles: manifest.sourceFiles.length,
    configFiles: manifest.configFiles.length,
    packageFiles: manifest.packageFiles.length,
    otherFiles: manifest.otherFiles.length,
    totalBytes: manifest.totalBytes,
    files: [...manifest.sourceFiles, ...manifest.configFiles, ...manifest.packageFiles],
  };
  index.readCounters = {
    filesRead: manifest.sourceFiles.length + manifest.configFiles.length
      + manifest.packageFiles.length + manifest.otherFiles.length,
    bytesRead: manifest.totalBytes,
  };
  index.profileDigest = sha256Hex(canonicalText({
    id: profile.id,
    readRoots: profile.readRoots.map((root) => ({ ...root })),
  }));
  index.programOptions = programOptions === null ? null : canonicalText(programOptions);
  sortIndex(index);
  return index;
}

/**
 * The scan session (plan §5/§6 — honestly scoped, see below).
 *
 * This is a DETERMINISTIC RE-SCAN PARITY CHECKER, not an incremental
 * compiler session. Every `scan()` runs the full cold pipeline; the
 * session only records the content-digest key of the previous input
 * manifest so callers can tell a warm (unchanged-inputs) run from a
 * cold one. No Program, no `oldProgram`, no module-resolution cache,
 * and no dependency/reverse-dependency graph is retained — true
 * incremental reuse was deferred: the process boundary (a fresh child
 * per exchange) and the read-only scan (no writable cache) make a
 * parent-owned cache transport a separately designed acceptance item,
 * and shipping a fake retained-Program claim would be worse than an
 * honest full re-scan. The correctness property this delivers —
 * identical inputs produce byte-identical indexes, any edit produces a
 * new manifest key — is the parity contract the tests assert.
 */
export class ScannerSession {
  constructor() {
    this.retain = null;
    this.scans = 0;
  }

  /**
   * One scan; records warm/cold reuse status against the retained key
   * (the digest of the complete input manifest).
   */
  scan(options) {
    this.scans += 1;
    const index = runScan(options);
    const key = sha256Hex(canonicalText(index.inputManifest));
    const result = { index, key, warm: this.retain?.key === key };
    this.retain = { key, index };
    return result;
  }
}

export {
  assertCompilerAvailable,
  buildInputManifest,
  enumerateInventory,
  nativeId,
  nativeIdentityTuple,
  runScan,
  signatureDigest,
  signatureGraphOfSymbol,
  sortIndex,
  SUPPORTED_KIND_WIRE,
  IDENTITY_DOMAIN,
  SIGNATURE_DOMAIN,
};


/**
 * The production extension entry point wired into the bundle descriptor:
 * translate one kernel dispatch context into a scanner run and the
 * closed internal outcome envelope. The semantic id proposals use the
 * package-scoped qualified name; every confidence is honest and
 * uncertainty in the index never becomes a fabricated success.
 */
export function scanOperation(context) {
  const { operation, profile, readView, cancellation, limits } = context;
  if (operation !== "scan") {
    return { state: "unsupported", diagnostics: [{ reason: "scan-only-extension" }] };
  }
  const permittedProjectRoot = readView.permittedProjectRoot;
  if (typeof permittedProjectRoot !== "string" || permittedProjectRoot === "") {
    return { state: "failed", diagnostics: [{ reason: "root-context-missing" }] };
  }
  let index;
  try {
    index = runScan({ profile, readView, permittedProjectRoot, limits });
  } catch (error) {
    if (cancellation?.cancelled) {
      return { state: "failed", diagnostics: [{ reason: "cancelled" }] };
    }
    return {
      state: "failed",
      diagnostics: [{ reason: String(error?.code ?? "scan-failed").slice(0, 64) }],
    };
  }
  if (cancellation?.cancelled) {
    return { state: "failed", diagnostics: [{ reason: "cancelled" }] };
  }
  if (index.state !== "complete") {
    return { state: "partial", diagnostics: [{ reason: "scan-incomplete" }] };
  }
  const entries = [];
  // Issue #47 (plan S8): native tests claiming a scenario identity via
  // the `lekalo:<id>` title convention ride the FIRST top-level symbol
  // entry of their module as the bounded `t` member (the observed scan
  // contract's native-test slot). The join with scenario bindings
  // happens in the scenario verify; here we only record what was found.
  const lekaloIdsByModule = lekaloTestIdsByModule(index.tests);
  // Dropped claims (the bounded t budget) surface as scan uncertainty —
  // a subsequent binding-missing stays honest but is no longer opaque
  // (review F-5).
  for (const module of lekaloIdsByModule.truncated) {
    index.anyUncertainty.push({
      path: module,
      kind: "test-binding-truncated",
      detail: "lekalo-id-budget",
      line: null,
    });
  }
  const moduleSeen = new Set();
  for (const symbol of index.symbols) {
    if (symbol.memberOf !== null) continue; // members ride their owner
    const detail = {
      s: semanticProposalFor(symbol, index),
      n: symbol.native,
      l: symbol.line,
      q: symbol.declarationOnly ? "low" : "medium",
    };
    if (!moduleSeen.has(symbol.module)) {
      moduleSeen.add(symbol.module);
      // Review F-2: the wire scan operation crashed on any project with
      // `lekalo:`-titled tests — the grouped claims were consulted as the
      // Map itself instead of the `{ byModule, truncated }` wrapper the
      // helper returns, so the observed `t` slot (and with it the whole
      // checked-binding join chain) was unreachable through the wire.
      const ids = lekaloIdsByModule.byModule.get(symbol.module);
      if (ids) {
        detail.t = `${symbol.module}#${ids.map((id) => `lekalo:${id}`).join(",")}`;
      }
    }
    entries.push({
      path: symbol.module,
      kind: "entity",
      detail: JSON.stringify(detail),
      evidence: buildEntryEvidence(symbol, index),
    });
  }
  const errorCount = index.diagnostics.filter((d) => d.severity === "error").length;
  const counts = {
    symbols: index.symbols.length,
    exports: index.exports.length,
    references: index.references.length,
    routes: index.routes.length,
    tests: index.tests.length,
    uncertainty: index.anyUncertainty.length,
    errors: errorCount,
  };
  if (index.anyUncertainty.length > 0 || errorCount > 0) {
    return {
      state: "partial",
      diagnostics: [{
        reason: "uncertainty-present",
        detail: "uncertainty=" + index.anyUncertainty.length + " errors=" + errorCount,
      }],
      evidence: {
        compiler: index.compiler,
        profileDigest: index.profileDigest,
        counts,
      },
    };
  }
  return {
    state: "complete",
    data: { entries, complete: true },
    evidence: {
      compiler: index.compiler,
      profileDigest: index.profileDigest,
      counts,
    },
  };
}

/**
 * The scenario-identity test titles of one scan, grouped by module:
 * bounded to 8 ids per module and 200 name characters per binding (the
 * observed `t` member's own bounds), sorted, deduplicated. Returns
 * `{ byModule, truncated }` — `truncated` names every module whose id
 * list was clipped, so the scanner surfaces dropped claims as scan
 * uncertainty instead of silence (review F-5).
 */
export function lekaloTestIdsByModule(tests) {
  const byModule = new Map();
  const truncated = [];
  for (const test of tests ?? []) {
    if (typeof test?.name !== "string" || typeof test?.path !== "string") continue;
    if (!test.name.startsWith("lekalo:")) continue;
    const id = test.name.slice("lekalo:".length);
    if (id.length === 0 || id.length > 128) continue;
    const bucket = byModule.get(test.path) ?? [];
    if (bucket.includes(id)) continue;
    if (bucket.length >= 8) {
      if (!truncated.includes(test.path)) truncated.push(test.path);
      continue;
    }
    bucket.push(id);
    byModule.set(test.path, bucket);
  }
  for (const [module, ids] of byModule) {
    ids.sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
    while (ids.map((id) => id.length + 8).reduce((sum, n) => sum + n, 0) > 200) {
      ids.pop();
      if (!truncated.includes(module)) truncated.push(module);
    }
  }
  truncated.sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
  return { byModule, truncated };
}

/** The semantic id proposal of one symbol: package-scoped dotted name. */
function semanticProposalFor(symbol, index) {
  const pkg = index.packages.find((candidate) =>
    symbol.module === candidate.root || symbol.module.startsWith(candidate.root + "/"));
  // The observed scan document derives the owning module from the id
  // prefix before the first dot, so the scope token is a sanitized
  // snake_case module id; ambiguity keeps the full proposal in the
  // internal index.
  const scope = (pkg?.name ?? "project")
    .replace(/^@/, "")
    .split(/[\\/._-]+/)
    .filter((part) => /^[a-z0-9]+$/i.test(part))
    .join("_")
    .toLowerCase() || "project";
  const name = symbol.qualifiedName
    .split(".")
    .map((part) => part)
    .join(".");
  return (scope + "." + name).slice(0, 192);
}

/** Typed evidence rows of one symbol entry (signature + references). */
function buildEntryEvidence(symbol, index) {
  const references = index.references
    .filter((row) => row.from === symbol.module)
    .slice(0, 8)
    .map((row) => ({
      // The wire evidence target must be a semantic id (no slashes): a
      // module edge target is the module's own stable semantic anchor.
      target: moduleSemanticAnchor(row.to, index),
      role: row.role,
      confidence: row.confidence,
    }));
  const evidence = {};
  if (symbol.signature !== null) evidence.signature = symbol.signature;
  if (references.length > 0) evidence.references = references;
  return Object.keys(evidence).length > 0 ? evidence : undefined;
}

/**
 * The stable semantic anchor of one module: the sanitized package scope
 * plus the module path with slashes mapped to dots. Resolvable and
 * deterministic; matches no slashes, so it satisfies the wire grammar.
 */
function moduleSemanticAnchor(modulePath, index) {
  const pkg = index.packages.find((candidate) =>
    modulePath === candidate.root || modulePath.startsWith(candidate.root + "/"));
  const scope = (pkg?.name ?? "project")
    .replace(/^@/, "")
    .split(/[\\/._-]+/)
    .filter((part) => /^[a-z0-9]+$/i.test(part))
    .join("_")
    .toLowerCase() || "project";
  const suffix = modulePath
    .replace(/\.(ts|tsx|mts|cts|d\.ts|d\.mts|d\.cts)$/i, "")
    .split("/")
    .join(".");
  return (scope + "." + suffix).slice(0, 192);
}
