// Node tool authoring belongs to the external TypeScript runtime.
import {pathToFileURL} from 'node:url';
import {join} from 'node:path';
import {externalRoot} from '../external-tool.mjs';
export const {createNodeToolCatalog, resolveNpmCli} = await import(pathToFileURL(join(externalRoot('lekalo-typescript'), 'scripts/lib/node-tool-catalog.mjs')).href);
