#!/usr/bin/env node

const { existsSync } = require("node:fs");
const path = require("node:path");
const { spawn } = require("node:child_process");

const binaryName = process.platform === "win32" ? "scoutly.exe" : "scoutly";
const binary = path.join(__dirname, "bin", binaryName);

if (!existsSync(binary)) {
  console.error("The Scoutly binary is missing. Reinstall @nelsonlaidev/scoutly.");
  process.exit(1);
}

const child = spawn(binary, process.argv.slice(2), { stdio: "inherit" });
child.on("error", (error) => {
  console.error(`Failed to run Scoutly: ${error.message}`);
  process.exit(1);
});
child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
