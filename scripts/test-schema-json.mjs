#!/usr/bin/env node
import assert from "node:assert/strict";
import {createSchemaJson} from "./lib/schema-json.mjs";

const io=createSchemaJson();
const source='{"minimum":-9223372036854775808,"maximum":9223372036854775807,"safe":9007199254740991,"text":"9223372036854775807","escaped":"quote \\" and 9223372036854775807","nested":[18446744073709551615]}';
const parsed=io.parse(source);
assert.equal(io.stringify(parsed),source);
assert.equal(io.stringify(structuredClone(parsed)),source);
const second=io.parse('{"maximum":9223372036854775807,"minimum":-9223372036854775808}');
assert.equal(io.stringify({copy:structuredClone(second)}),'{"copy":{"maximum":9223372036854775807,"minimum":-9223372036854775808}}');
assert.equal(io.stringify({text:parsed.text}),' {"text":"9223372036854775807"}'.trim());
for(const text of ['{"value":"__lekalo_schema_integer_0__"}','{"value":"\\u005f_lekalo_schema_integer_0__"}']) assert.throws(()=>io.parse(text),/reserved schema integer marker/);
assert.throws(()=>io.parse('{"maximum":9223372036854775807,}'));
console.log(JSON.stringify({ok:true,gate:"schema-json",controls:7,integerLexemesPreserved:true}));
