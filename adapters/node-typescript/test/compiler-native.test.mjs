import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtempSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createTestCompiler} from './compiler-test-runtime.mjs';
import {parseJsonConfig} from '../src/compiler.mjs';
import {ast} from '../src/compiler-client.mjs';

test('JSONC preserves escaped comment-like text and rejects malformed structure', () => {
  const value = parseJsonConfig('{/*comment*/"url":"https://example.test/a/*b*/", "values":[-2,null,false,],}', ast);
  assert.equal(value.url, 'https://example.test/a/*b*/');
  assert.deepEqual(value.values, [-2, null, false]);
  for (const text of ['{} trailing', '{"x":}', '{"x":"unterminated}', '{} /*unterminated', '{"x":0x10}', '{"x":NaN}', '{"x":1,,}']) {
    assert.throws(() => parseJsonConfig(text, ast), text);
  }
  const prototype = parseJsonConfig('{"__proto__":{"unsafe":true}}', ast);
  assert.equal(Object.getPrototypeOf(prototype), null);
  assert.equal({}.unsafe, undefined);
});

function hostFor(files) {
  return {readFile: path => files.get(path), fileExists: path => files.has(path),
    getCurrentDirectory: () => '/lekalo/project',
    directoryExists: path => [...files.keys()].some(file => file.startsWith(path.replace(/\/$/, '') + '/')),
    readDirectory: () => [...files.keys()], useCaseSensitiveFileNames: () => false};
}

test('admitted project declarations named lib.*.d.ts remain project sources', () => {
  for (const name of ['lib.domain.d.ts', 'lib.es5.d.ts']) {
    const ts = createTestCompiler();
    try {
      const path = `/lekalo/project/${name}`;
      const files = new Map([
        ['/lekalo/project/main.ts', `import type {Rule} from "./${name.slice(0, -5)}"; export const value: Rule = {name: "local"};`],
        [path, 'export interface Rule { name: string }'],
      ]);
      const program = ts.createProgram({rootNames: [...files.keys()], options: {noEmit: true, noLib: true,
        module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler}, host: hostFor(files)});
      assert.deepEqual(program.getSourceFiles().map(file => file.fileName).sort(), [...files.keys()].sort());
      assert.equal(program.getSourceFile(path).text, files.get(path));
      assert.equal(program.isSourceFileDefaultLibrary(program.getSourceFile(path)), false);
      assert.deepEqual(program.getSemanticDiagnostics(), []);
    } finally { ts.close(); }
  }
});

test('native compiler cannot resolve a real host file omitted by the admitted VFS', () => {
  const home = mkdtempSync(join(tmpdir(), 'lekalo-ts7-vfs-canary-'));
  const sentinel = join(home, 'secret.ts').replaceAll('\\', '/');
  writeFileSync(sentinel, 'export const secret = "HOST_CANARY_MUST_NOT_ENTER_THE_INDEX";');
  const ts = createTestCompiler();
  try {
    const files = new Map([
      ['/lekalo/project/main.ts', `import { secret } from ${JSON.stringify(sentinel)}; import { allowed } from './allowed'; export { allowed }; export const result = secret;`],
      ['/lekalo/project/allowed.ts', 'export const allowed: number = 1;'],
    ]);
    const program = ts.createProgram({rootNames: ['/lekalo/project/main.ts'], options: {noEmit: true, noLib: true,
      module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler}, host: hostFor(files)});
    assert.deepEqual(program.getSourceFiles().map(file => file.fileName).sort(), [...files.keys()].sort());
    const diagnostics = program.getSemanticDiagnostics();
    assert.ok(diagnostics.some(row => row.code === 2307 && row.messageText.includes('secret')));
    assert.ok(!diagnostics.some(row => row.code === 2307 && row.messageText.includes('./allowed')));
  } finally { ts.close(); rmSync(home, {recursive: true}); }
});

test('native configuration errors are preserved instead of becoming default options', () => {
  const ts = createTestCompiler();
  try {
    const raw = {compilerOptions: {madeUpCompilerOption: true, moduleResolution: 'node10'}, files: ['main.ts']};
    const files = new Map([['/lekalo/project/main.ts', 'export const value=1;'], ['/lekalo/project/tsconfig.json', JSON.stringify(raw)]]);
    const host = hostFor(files);
    const parsed = ts.parseJsonConfigFileContent(raw, host, '/lekalo/project', undefined, '/lekalo/project/tsconfig.json');
    assert.ok(parsed.errors.some(row => row.code === 5023), JSON.stringify(parsed.errors));
    const program = ts.createProgram({rootNames: parsed.fileNames, options: {...parsed.options, noLib: true, noEmit: true}, host});
    assert.ok(program.getOptionsDiagnostics().some(row => row.code === 5108), 'removed module resolution must refuse');
  } finally { ts.close(); }
});

test('inherited path mappings retain the directory of the declaring config', () => {
  const ts = createTestCompiler();
  try {
    const parent = {compilerOptions: {module: 'esnext', moduleResolution: 'bundler', target: 'esnext',
      noLib: true, paths: {shared: ['./value.ts']}}};
    const raw = {extends: '../base/tsconfig.json', files: ['main.ts']};
    const files = new Map([
      ['/lekalo/project/base/tsconfig.json', JSON.stringify(parent)],
      ['/lekalo/project/base/value.ts', 'export const value: number = 1;'],
      ['/lekalo/project/child/tsconfig.json', JSON.stringify(raw)],
      ['/lekalo/project/child/main.ts', 'import {value} from "shared"; export {value};'],
    ]);
    const host = hostFor(files);
    const parsed = ts.parseJsonConfigFileContent(raw, host, '/lekalo/project/child', undefined, '/lekalo/project/child/tsconfig.json');
    assert.deepEqual(parsed.errors, []);
    assert.deepEqual(parsed.options.paths.shared, ['/lekalo/project/base/value.ts']);
    const program = ts.createProgram({rootNames: parsed.fileNames, options: {...parsed.options, noEmit: true}, host});
    assert.ok(!program.getSemanticDiagnostics().some(row => row.code === 2307));
    assert.deepEqual(program.getOptionsDiagnostics(), []);
    assert.deepEqual(program.getConfigFileParsingDiagnostics(), []);
  } finally { ts.close(); }
});
