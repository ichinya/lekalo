#!/usr/bin/env node
/** Build/install-time only. Copy the reviewed host compiler; no downloads,
 * package-manager execution, PATH lookup or runtime installation fallback. */
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync, readdirSync, mkdirSync, lstatSync, writeFileSync, chmodSync, existsSync} from 'node:fs';
import {dirname, join} from 'node:path';
import {fileURLToPath} from 'node:url';
const root = dirname(fileURLToPath(import.meta.url));
const pins = JSON.parse(readFileSync(join(root, 'compiler-resources.json')));
const resource = pins.resources.find(row => row.platform === `${process.platform}-${process.arch}`);
assert.ok(resource, 'unsupported native compiler platform');
const path = join(root, 'node_modules', resource.package, resource.file);
assert.ok(lstatSync(path).isFile() && !lstatSync(path).isSymbolicLink(), 'native compiler must be a regular file');
const bytes = readFileSync(path);
assert.equal(bytes.length, resource.bytes);
assert.equal(createHash('sha256').update(bytes).digest('hex'), resource.sha256, 'native compiler integrity');
const libraryRoot = dirname(path);
const libraries = readdirSync(libraryRoot).filter(name => /^lib(\..+)?\.d\.ts$/.test(name)).sort().map(name => {
  const data = readFileSync(join(libraryRoot, name));
  return {path: name, sha256: createHash('sha256').update(data).digest('hex'), bytes: data.length, data};
});
assert.equal(libraries.length, pins.standardLibraries.files);
assert.equal(createHash('sha256').update(JSON.stringify(libraries.map(({data, ...row}) => row))).digest('hex'), pins.standardLibraries.sha256);
mkdirSync(join(root, 'runtime', 'libraries'), {recursive: true});
for (const library of libraries) writeFileSync(join(root, 'runtime', 'libraries', library.path), library.data);
const destination = join(root, 'runtime', resource.platform, resource.file.split('/').at(-1));
mkdirSync(dirname(destination), {recursive: true});
if (existsSync(destination)) {
  assert.ok(lstatSync(destination).isFile() && !lstatSync(destination).isSymbolicLink());
  assert.ok(readFileSync(destination).equals(bytes), 'refusing to replace a modified compiler resource');
} else writeFileSync(destination, bytes, {mode: 0o500, flag: 'wx'});
if (process.platform !== 'win32') chmodSync(destination, 0o500);
console.log(JSON.stringify({ok: true, compiler: pins.typescript, platform: resource.platform, sha256: resource.sha256, bytes: bytes.length}));
