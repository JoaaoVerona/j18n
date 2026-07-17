#!/usr/bin/env node

const path = require('node:path');
const { spawnSync } = require('node:child_process');
const KEY = process.platform + '-' + process.arch;
const BINARIES = {
  "linux-x64": "j18n",
  "linux-arm64": "j18n",
  "darwin-arm64": "j18n",
  "darwin-x64": "j18n",
  "win32-x64": "j18n.exe",
  "win32-arm64": "j18n.exe"
};
const bin = BINARIES[KEY];

if (!bin) {
	console.error(`j18n: unsupported platform ${KEY}. Supported: ${Object.keys(BINARIES).join(', ')}`);
	process.exit(1);
}

const binPath = path.join(__dirname, KEY, bin);
const result = spawnSync(binPath, process.argv.slice(2), { stdio: 'inherit' });

if (result.error) {
	console.error(`j18n: failed to spawn ${binPath}: ${result.error.message}`);
	process.exit(1);
}

process.exit(result.status == null ? 1 : result.status);
