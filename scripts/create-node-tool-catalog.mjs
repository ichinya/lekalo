#!/usr/bin/env node
// Usage: node scripts/create-node-tool-catalog.mjs /physical/store/node_modules
import {createNodeToolCatalog} from './lib/node-tool-catalog.mjs';
if(process.argv.length!==3)throw new Error('usage: create-node-tool-catalog.mjs NODE_MODULES');
process.stdout.write(JSON.stringify(createNodeToolCatalog(process.argv[2]),null,2)+'\n');
