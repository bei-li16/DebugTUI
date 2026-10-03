// npm install creates the user-owned catalogue without replacing existing data.
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const result = spawnSync(path.join(__dirname, '..', 'bin', 'debugtui.exe'), ['--init-profiles'], {
  encoding: 'utf8', windowsHide: true,
});
if (result.status !== 0) {
  console.error('Device catalogue initialization failed. Run debugtui --init-profiles after correcting the reported path or file.');
  console.error(result.error?.message || result.stderr || 'Unknown initialization failure');
  process.exit(1);
}
process.stdout.write(result.stdout);
