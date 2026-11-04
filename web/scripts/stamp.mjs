import { execSync } from "node:child_process";
import { writeFileSync } from "node:fs";

let sha = "dev";
try {
  sha = execSync("git rev-parse --short HEAD", { encoding: "utf8" }).trim();
} catch {}
const build = `${sha}-${Date.now()}`;
writeFileSync(new URL("../public/version.json", import.meta.url), JSON.stringify({ build }) + "\n");
console.log(`stamped ${build}`);
