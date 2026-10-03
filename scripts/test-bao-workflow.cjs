// Explicit hardware regression: builds and flashes the Bao smoke image.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net');
const assert = require('node:assert/strict');
const {quote, hash, delay, outputDirectory, Cases, Session} = require('./test-support/session.cjs');
const [binaryArg, baoArg, flag, coresArg = '0,1'] = process.argv.slice(2);
assert(binaryArg && baoArg && flag === '--flash' && ['0', '0,1'].includes(coresArg),
  'Usage: BINARY BAO_ROOT --flash [0|0,1] (replaces board firmware)');
const coreNames = coresArg.split(',').map(id => `core.${id}`);
const binary = path.resolve(binaryArg), bao = path.resolve(baoArg);
const out = outputDirectory('bao-workflow'), suite = new Cases(out, {binary, binarySha256: hash(binary), bao, coreNames});
const protectedFiles = ['debug.toml', 'debug-dual.toml', 'scripts/tha6206/debug-env.toml'].map(file => path.join(bao, file));
const before = protectedFiles.map(hash);
let config = fs.readFileSync(protectedFiles[0], 'utf8');
// Keep user's project untouched; omit saved breakpoints from this controlled test.
config = config.replace(/^\[\[breakpoints\]\][\s\S]*?(?=^\[|$(?![\s\S]))/gm, '')
  .replace(/^\[\[?core_preferences[^\n]*\n[\s\S]*?(?=^\[|$(?![\s\S]))/gm, '')
  .replace('profile = "scripts/tha6206/debug-env.toml"', `profile = ${quote(path.join(bao, 'scripts/tha6206/debug-env.toml'))}`)
  .replace('elf = "bin/tha6xxx/tha6206-smoke/bao.elf"', `elf = ${quote(path.join(bao, 'bin/tha6xxx/tha6206-smoke/bao.elf'))}`)
  .replace('source_root = "."', `source_root = ${quote(bao)}`)
  .replace('svd = "../THA6XXX_MC_AS440/.vscode/THA6206/tha6206.svd"', '')
  .replace('on_exit = "detach"', 'on_exit = "resume"');
const project = path.join(out, 'debug.toml'); fs.writeFileSync(project, config);
function rpc(command) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection({host: '127.0.0.1', port: 6666}); let text = '';
    socket.setTimeout(20000); socket.on('timeout', () => socket.destroy(Error('OpenOCD timeout')));
    socket.on('error', reject); socket.on('connect', () => socket.write(command + '\x1a'));
    socket.on('data', data => {text += data.toString(); if (text.includes('\x1a')) {socket.end(); resolve(text.split('\x1a')[0]);}});
  });
}
async function verifyFlash() {
  const expected = fs.readFileSync(path.join(bao, 'bin/tha6xxx/tha6206-smoke/bao.bin'));
  const chunks = [];
  for (let offset = 0; offset < expected.length; offset += 2048) {
    const count = Math.min(2048, expected.length - offset);
    const words = (await rpc(`AHB_3 read_memory ${0x90400000 + offset} 8 ${count}`)).trim().split(/\s+/);
    assert.equal(words.length, count);
    chunks.push(Buffer.from(words.map(word => {const n = Number(word); assert(n >= 0 && n <= 255); return n;})));
  }
  const actual = Buffer.concat(chunks); fs.writeFileSync(path.join(out, 'flash.bin'), actual);
  assert(actual.equals(expected), 'Flash differs from the newly built Bao binary');
  return {bytes: actual.length, sha256: hash(path.join(out, 'flash.bin'))};
}
(async () => {
  const s = new Session(binary, project, out, ['--chip', 'tha6206', '--cores', coresArg]);
  try {
    // The caller has installed the smoke image first; Start now uses matching symbols.
    if (!await suite.test('connect', 'Connect selected cores with Bao source symbols', async () => {
      const result = await s.command('connect', {}, true, 65000);
      assert.deepEqual(result.core_names, coreNames); return result.core_names;
    })) return;
    if (!await suite.test('build-connected', 'Build releases selected cores and the probe, then reloads symbols', async () => {
      const result = await s.command('build', {}, true, 300000);
      assert(fs.existsSync(path.join(bao, 'bin/tha6xxx/tha6206-smoke/bao.sha256')));
      const status = await s.command('status'); assert(status.cores.every(core => core.state === 'STOPPED'));
      return result;
    })) return;
    if (!await suite.test('download-connected', 'Download from DebugTUI releases the probe and reconnects selected cores', async () => {
      const result = await s.command('download', {}, true, 300000);
      const status = await s.command('status'); assert(status.cores.every(core => core.state === 'STOPPED'));
      return result;
    })) return;
    if (!await suite.test('flash-readback', 'Every programmed Flash byte matches this build', verifyFlash)) return;
    await suite.test('debug-after-download', 'Run to C source, step, continue, and check Guest heartbeat', async () => {
      await s.command('run'); await s.command('wait_stopped', {timeout_ms: 15000});
      await s.command('select_core', {name: 'core.0'});
      let status = await s.command('status');
      assert.equal(status.frame.function, 'init'); assert(status.frame.file.endsWith('init.c'));
      const source = status.frame.file;
      await s.command('step'); await s.command('wait_stopped');
      await s.command('continue'); await delay(800);
      const first = (await rpc('AHB_3 read_memory 0x1003f000 32 6')).trim().split(/\s+/).map(Number);
      await delay(300);
      const second = (await rpc('AHB_3 read_memory 0x1003f000 32 6')).trim().split(/\s+/).map(Number);
      assert.equal(first[0], 0x42414f47); assert.equal(first[4], 0); assert.notEqual(first[5], second[5]);
      await s.command('pause'); status = await s.command('status');
      assert(status.cores.every(core => core.state === 'STOPPED')); await s.command('continue');
      return {source, marker: first[0], heartbeat: [first[5], second[5]]};
    });
  } finally {await s.close(); assert.deepEqual(protectedFiles.map(hash), before); suite.finish();}
})().catch(error => {console.error(error); process.exitCode = 1;});
