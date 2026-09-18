// Inert synthetic native-test text (issue #43 fixture).
// Never executed by the #43 kernel; #48 owns native gates.
import { fixtureGreeting, FIXTURE_LIMIT } from "../src/example.ts";

const results: string[] = [];
for (let index = 0; index < FIXTURE_LIMIT; index += 1) {
  results.push(fixtureGreeting(`case-${index}`));
}

if (results.length !== FIXTURE_LIMIT) {
  throw new Error("synthetic fixture invariant");
}
