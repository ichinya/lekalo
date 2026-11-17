#!/usr/bin/env node
import {execFileSync} from 'node:child_process';
import {join} from 'node:path';
import {hostRoot,runExternal} from './external-tool.mjs';
execFileSync(process.execPath,['--test',join(hostRoot,'scripts/test-external-tool.mjs')],{stdio:'inherit'});
runExternal('lekalo-adapter-sdk','scripts/test-adapter-sdk-contracts.mjs');
