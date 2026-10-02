import { readFileSync, writeFileSync } from "node:fs";
const p = "crates/lekalo-cli/src/main.rs";
let t = readFileSync(p, "utf8");
const bs = String.fromCharCode(92);
const q = String.fromCharCode(34);
const esc = (s) => s.split(q).join(bs + q);
const old1 = [
  "            let json = if let Some(comparison_json) = comparison_json {",
  "                format!(",
  '                    "' + "{{" + esc('"status":"valid","contextBudget":{},"contextBudgetComparison":{}') + "}}" + '",',
  "                    canonical, comparison_json",
  "                )",
  "            } else {",
  "                format!(",
  '                    "' + "{{" + esc('"status":"valid","contextBudget":{}') + "}}" + '",',
  "                    canonical",
  "                )",
  "            };",
].join("\n");
const new1 = [
  "            let build_envelope = |comparison_json: &Option<String>| {",
  "                if let Some(comparison_json) = comparison_json {",
  "                    format!(",
  '                        "' + "{{" + esc('"status":"valid","contextBudget":{},"contextBudgetComparison":{}') + "}}" + '",',
  "                        canonical,",
  "                        comparison_json",
  "                    )",
  "                } else {",
  "                    format!(",
  '                        "' + "{{" + esc('"status":"valid","contextBudget":{}') + "}}" + '",',
  "                        canonical",
  "                    )",
  "                }",
  "            };",
].join("\n");
if (!t.includes(old1)) { console.error("e1"); process.exit(1); }
t = t.replace(old1, new1);
const old2 = "                    return DomainResult::denied_json(json, human, denial);";
const new2 = "                    return DomainResult::denied_json(build_envelope(&comparison_json), human, denial);";
if (!t.includes(old2)) { console.error("e2"); process.exit(1); }
t = t.replace(old2, new2);
const old3 = "            DomainResult::graph(json, human, diagnostics)";
const new3 = "            DomainResult::graph(build_envelope(&comparison_json), human, diagnostics)";
if (!t.includes(old3)) { console.error("e3"); process.exit(1); }
t = t.replace(old3, new3);
writeFileSync(p, t);
console.log("envelope deferred");
