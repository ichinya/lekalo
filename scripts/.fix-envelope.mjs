import { readFileSync, writeFileSync } from "node:fs";
const p = "crates/lekalo-cli/src/main.rs";
const q = String.fromCharCode(34);
let lines = readFileSync(p, "utf8").split("\n");
const fmtLine = '                "' + "{{" + q + "status" + q + ":" + q + "valid" + q + "," + q + "contextBudget" + q + ":{}}}" + q + ",";
const startIdx = lines.findIndex(
  (line, i) =>
    line === "            let json = format!(" &&
    lines[i + 1] === fmtLine &&
    lines[i + 2] === "                canonical" &&
    lines[i + 3] === "            );",
);
if (startIdx === -1) {
  console.error("block not found");
  process.exit(1);
}
const fmtLine2 =
  '                    "' + "{{" + q + "status" + q + ":" + q + "valid" + q + "," + q + "contextBudget" + q + ":{}," + q + "contextBudgetComparison" + q + ":{}}}" + q + ",";
const fmtLine3 =
  '                    "' + "{{" + q + "status" + q + ":" + q + "valid" + q + "," + q + "contextBudget" + q + ":{}}}" + q + ",";
const newBlock = [
  "            let json = if let Some(comparison_json) = comparison_json {",
  "                format!(",
  fmtLine2,
  "                    canonical,",
  "                    comparison_json",
  "                )",
  "            } else {",
  "                format!(",
  fmtLine3,
  "                    canonical",
  "                )",
  "            };",
];
lines.splice(startIdx, 4, ...newBlock);
writeFileSync(p, lines.join("\n"));
console.log("envelope wired at line", startIdx + 1);
