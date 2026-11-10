/** Authoring-only compiler harness for generated-code regression tests. Uses
 * the real TS7 compiler and filesystem, outside product confinement. */
import {API, DiagnosticCategory, ModuleKind} from '../node_modules/typescript/dist/api/sync/api.js';
import {ast, ModuleResolutionKind} from '../src/compiler-client.mjs';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdtempSync, readFileSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const pins = JSON.parse(readFileSync(join(root, 'compiler-resources.json')));
const resource = pins.resources.find(row => row.platform === `${process.platform}-${process.arch}`);
const binary = join(root, 'node_modules', resource.package, resource.file);
const bytes = readFileSync(binary);
if (bytes.length !== resource.bytes || createHash('sha256').update(bytes).digest('hex') !== resource.sha256) throw new Error('native typecheck compiler integrity');
const optionsJson = input => {
  const options = {...input};
  for (const [key, values] of Object.entries({target: ast.ScriptTarget, module: ModuleKind, moduleResolution: ModuleResolutionKind})) {
    if (typeof options[key] === 'number') options[key] = values[options[key]].toLowerCase();
  }
  if (options.baseUrl !== undefined) {
    throw new Error('TypeScript 7 removed baseUrl; use explicit path mappings');
  }
  return options;
};
export const nativeTestCompiler = {
  version: pins.typescript, ScriptTarget: ast.ScriptTarget, ModuleKind, ModuleResolutionKind, DiagnosticCategory,
  createCompilerHost: () => ({}),
  createProgram: (files, options) => ({files, options}),
  flattenDiagnosticMessageText: text => String(text),
  formatDiagnostics: rows => rows.map(row => `${row.fileName ?? ''} TS${row.code}: ${row.messageText}`).join('\n'),
  getPreEmitDiagnostics({files, options}) {
    const home = mkdtempSync(join(tmpdir(), 'lekalo-ts7-typecheck-')), config = join(home, 'tsconfig.json');
    writeFileSync(config, JSON.stringify({compilerOptions: optionsJson(options), files}));
    const api = new API({tsserverPath: binary, cwd: home});
    try {
      const project = api.updateSnapshot({openProjects: [config]}).getProject(config), program = project.program;
      const rows = [...program.getConfigFileParsingDiagnostics(), ...program.getProgramDiagnostics(),
        ...program.getGlobalDiagnostics(), ...program.getSyntacticDiagnostics(), ...program.getSemanticDiagnostics()];
      return [...new Map(rows.map(row => [JSON.stringify(row), {...row, messageText: row.text}])).values()];
    } finally { api.close(); rmSync(home, {recursive: true}); }
  },
  transpileModule(text, {compilerOptions}) {
    const home = mkdtempSync(join(tmpdir(), 'lekalo-ts7-emit-'));
    try {
      writeFileSync(join(home, 'input.ts'), text);
      writeFileSync(join(home, 'tsconfig.json'), JSON.stringify({compilerOptions: {...optionsJson(compilerOptions), types: [], outDir: 'out'}, files: ['input.ts']}));
      const result = spawnSync(binary, ['--project', join(home, 'tsconfig.json'), '--pretty', 'false'], {encoding: 'utf8', timeout: 30000, windowsHide: true});
      if (result.status !== 0) throw new Error(result.stdout + result.stderr);
      return {outputText: readFileSync(join(home, 'out', 'input.js'), 'utf8')};
    } finally { rmSync(home, {recursive: true}); }
  },
};
