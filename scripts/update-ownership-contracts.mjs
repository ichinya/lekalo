#!/usr/bin/env node
// Explicit maintenance; verification never regenerates contracts.
import {writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {ownershipSchemas} from './lib/ownership-contracts.mjs';
if(process.argv.slice(2).join(' ') !== '--write') throw Error('usage: --write');
for(const [family,schema] of Object.entries(ownershipSchemas)) {
  writeFileSync(fileURLToPath(new URL(`../contracts/${family}.schema.v1.0.0.json`,import.meta.url)),JSON.stringify(schema,null,2)+'\n');
}
