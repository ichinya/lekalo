#!/usr/bin/env node
import {runExternal} from "./external-tool.mjs";
runExternal("lekalo-adapter-sdk", "scripts/gen-adapter-sdk-fixtures.mjs");
