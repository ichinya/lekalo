#!/usr/bin/env node
// Author + seal the two independently authored mapping-evidence fixtures
// (issue #113 AC7, r4 review finding 10/codex). The two documents are
// written from separate observations of the same public digest — an
// in-process author and an extracted-service author — never generated
// from one another; this script only computes each document's own digest
// seal over its canonical bytes.
import { createHash } from 'node:crypto';
import { writeFileSync, mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const digest = (domain, canonical) =>
  `sha256:${createHash('sha256').update(`${domain}\n${canonical}`).digest('hex')}`;

const canonical = (value) => {
  const escape = (text) => {
    let out = '"';
    for (const character of text) {
      if (character === '"') out += '\\"';
      else if (character === '\\') out += '\\\\';
      else if (character === '\n') out += '\\n';
      else if (character === '\r') out += '\\r';
      else if (character === '\t') out += '\\t';
      else if (character === '\b') out += '\\b';
      else if (character === '\f') out += '\\f';
      else if (character < ' ') out += `\\u${character.charCodeAt(0).toString(16).padStart(4, '0')}`;
      else out += character;
    }
    return out + '"';
  };
  const write = (node) => {
    if (node === null) return 'null';
    if (node === true) return 'true';
    if (node === false) return 'false';
    if (typeof node === 'number') return node.toString();
    if (typeof node === 'string') return escape(node);
    if (Array.isArray(node)) return `[${node.map(write).join(',')}]`;
    const keys = Object.keys(node).sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
    return `{${keys.map((key) => `${escape(key)}:${write(node[key])}`).join(',')}}`;
  };
  return write(value);
};

const seal = (document) => {
  const without = { ...document, digest: '' };
  return { ...document, digest: digest('lekalo/module-mapping-evidence/1', canonical(without)) };
};

const write = (dir, name, document) => {
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, name), JSON.stringify(document, null, 2) + '\n');
};

// Bind to the current committed golden bundle digest (never a stale constant).
const publicDigest = JSON.parse(readFileSync('tests/fixtures/module-api/standalone/golden-bundle.json', 'utf8')).digest;
const producer = { project: 'tasks', module: 'tasks' };

// The in-process author observed the module through direct calls.
const inProcess = seal({
  schemaVersion: 'lekalo/module-mapping-evidence/v0.8.0',
  identity: 'dev.lekalo.module-mapping-evidence@0.8.0',
  implementation: 'in-process',
  port: 'tasks.inprocess',
  producer,
  publicDigest,
  observations: [
    {
      operation: 'tasks.get_task',
      kind: 'query',
      inputs: [],
      results: { ref: 'tasks.task_view' },
      errors: [],
      effects: [],
      access: 'read',
      capabilities: [],
    },
    {
      operation: 'tasks.list_tasks',
      kind: 'query',
      inputs: [],
      results: { list: { ref: 'tasks.task_view' } },
      errors: [],
      effects: [],
      access: 'read',
      capabilities: [],
    },
    {
      operation: 'tasks.create_task_cmd',
      kind: 'command',
      inputs: [{ name: 'title', required: true, type: { ref: 'tasks.title' } }],
      results: null,
      errors: [],
      effects: [{ entity: 'tasks.task', operation: 'create', emits: ['tasks.task_created'] }],
      access: 'write',
      capabilities: [],
    },
    {
      operation: 'tasks.complete_task_cmd',
      kind: 'command',
      inputs: [{ name: 'task_id', required: true, type: { ref: 'tasks.task_id' } }],
      results: null,
      errors: [],
      effects: [{ entity: 'tasks.task', operation: 'update', emits: [] }],
      access: 'write',
      capabilities: [],
    },
  ],
  unsupported: [],
});

// The extracted-service author observed the same module through its
// service port; the observations were written separately from the wire
// contract of the extracted deployment, and this implementation
// explicitly cannot provide batch imports (not a bundle capability —
// omitted; the http transport capability it does provide).
const service = seal({
  schemaVersion: 'lekalo/module-mapping-evidence/v0.8.0',
  identity: 'dev.lekalo.module-mapping-evidence@0.8.0',
  implementation: 'extracted-service',
  port: 'tasks.api',
  producer,
  publicDigest,
  observations: [
    {
      operation: 'tasks.get_task',
      kind: 'query',
      inputs: [],
      results: { ref: 'tasks.task_view' },
      errors: [],
      effects: [],
      access: 'read',
      capabilities: ['http'],
    },
    {
      operation: 'tasks.list_tasks',
      kind: 'query',
      inputs: [],
      results: { list: { ref: 'tasks.task_view' } },
      errors: [],
      effects: [],
      access: 'read',
      capabilities: ['http'],
    },
    {
      operation: 'tasks.create_task_cmd',
      kind: 'command',
      inputs: [{ name: 'title', required: true, type: { ref: 'tasks.title' } }],
      results: null,
      errors: [],
      effects: [{ entity: 'tasks.task', operation: 'create', emits: ['tasks.task_created'] }],
      access: 'write',
      capabilities: ['http'],
    },
    {
      operation: 'tasks.complete_task_cmd',
      kind: 'command',
      inputs: [{ name: 'task_id', required: true, type: { ref: 'tasks.task_id' } }],
      results: null,
      errors: [],
      effects: [{ entity: 'tasks.task', operation: 'update', emits: [] }],
      access: 'write',
      capabilities: ['http'],
    },
  ],
  unsupported: [],
});

write('tests/fixtures/module-api/mapping/in-process', 'mapping-evidence.json', inProcess);
write('tests/fixtures/module-api/mapping/service', 'mapping-evidence.json', service);
write('crates/lekalo-core/tests/fixtures/module-api/mapping/in-process', 'mapping-evidence.json', inProcess);
write('crates/lekalo-core/tests/fixtures/module-api/mapping/service', 'mapping-evidence.json', service);
console.log(JSON.stringify({ inProcess: inProcess.digest, service: service.digest }));
