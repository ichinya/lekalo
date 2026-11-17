#!/usr/bin/env node
import {runExternal} from "./external-tool.mjs";
runExternal("lekalo-typescript-zod", "scripts/gen-zod-golden.mjs");
