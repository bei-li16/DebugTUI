#!/usr/bin/env node
'use strict';
// Actual CLI/configuration and MI dispatch; no board access or flash operation.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {root, quote, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary']);
const binary = path.resolve(options.binary || path.join(root, 'target/debug/debugtui.exe'));
const out = outputDirectory('host-descriptions');
const user = path.join(out, '用户配置'), project = path.join(out, 'debug.toml');
const previous = process.env.DEBUGTUI_CONFIG_DIR;
process.env.DEBUGTUI_CONFIG_DIR = user;
const suite = new Cases(out, {binary, binary_sha256:hash(binary), board_tests_executed:false});
let sequence = 0;
const base = "version=3\n[tools]\nprofile='builtin:arm-openocd'\n[debug]\nchip='stm32f429'\ncores=[0]\n";
const generic = "version=3\n[tools]\nprofile='builtin:generic-gdb'\n";
function listing(text, expected = true) {
  fs.writeFileSync(project, text);
  const before = hash(project);
  const r = spawnSync(binary, ['--project', project, '--headless', '--stdio'], {
    input:'{"id":1,"method":"registers_list"}\n{"id":2,"method":"quit"}\n',
    encoding:'utf8', windowsHide:true, timeout:30000, maxBuffer:32*1024*1024
  });
  fs.writeFileSync(path.join(out, `listing-${++sequence}.json`), JSON.stringify(r, null, 2));
  assert.ifError(r.error);
  assert.equal(hash(project), before);
  if (!expected) { assert.notEqual(r.status, 0); return r.stderr; }
  assert.equal(r.status, 0, r.stderr);
  const events = r.stdout.trim().split(/\r?\n/).map(JSON.parse);
  assert(!events.some(e => e.event === 'log' && e.channel === 'mi>'));
  const response = events.find(e => e.id === 1 && e.event === 'response');
  assert.equal(response?.ok, true, JSON.stringify(response));
  return response.result;
}
(async () => {
  try {
    await suite.test('HOST-F429-AUTO', 'F429 automatically selects 617 definitions and owner-scoped PPB/DBGMCU mappings', async () => {
      const list = listing(base);
      assert.equal(list.catalogue.registers.length, 617);
      for (const id of ['scb.cpuid','scb.vtor','dbgmcu.idcode','pc','sp','xpsr']) assert(list.catalogue.registers.some(r => r.id === id), id);
      const settings = list.configuration.settings;
      assert(settings.some(s => s.path.join('/') === 'component_owners/ppb/core:core.0/channel' && s.value === 'core-tcl'));
      assert(settings.some(s => s.path.join('/') === 'component_owners/dbgmcu/chip:stm32f429/base' && s.value === 0xe0042000));
      assert(settings.some(s => s.path[0] === 'catalogue' && s.source.startsWith('chip default:')));
      return {count:list.catalogue.registers.length, source:list.source};
    });
    await suite.test('HOST-EXPLICIT', 'Explicit project CPU and GDB-only selections override the chip default', async () => {
      assert.equal(listing(base + "[registers]\ncpu=''\ncatalogue=''\n").source, 'gdb');
      const list = listing(base + "[registers]\ncpu='cortex-m3'\n");
      assert.equal(list.catalogue.cpu, 'cortex-m3');
      assert.equal(list.catalogue.registers.length, 330);
    });
    const index = path.join(user, 'profiles/devices.toml'), indexBytes = fs.readFileSync(index), indexHash = hash(index);
    await suite.test('HOST-GENERIC', 'Unknown chips can use generic GDB even when the legacy device index is broken', async () => {
      fs.writeFileSync(index, 'broken=[');
      try {
        const list = listing(generic + "[debug]\nchip='uninstalled-f103'\ncores=[0]\n");
        assert.equal(list.source, 'gdb');
        assert.equal(list.catalogue, null);
      } finally { fs.writeFileSync(index, indexBytes); }
    });
    const chip = path.join(user, 'profiles/chips/stm32f103.toml');
    const registers = path.join(user, 'profiles/registers/stm32f103.toml');
    const f103 = base.replace('stm32f429', 'stm32f103');
    await suite.test('HOST-DROP-IN', 'Adding two description files enables a new chip without editing devices.toml', async () => {
      assert(listing(f103, false).includes('stm32f103'));
      fs.writeFileSync(chip, "[chip]\ncores=[0]\nbackend='stm32f1'\nregister_catalogue='../registers/stm32f103.toml'\n[core_targets.\"0\"]\nendpoint='localhost:3333'\n");
      fs.writeFileSync(registers, "version=1\ncpu='test-f103'\narchitecture='armv7-m'\nextends=['cortex-m3']\n");
      const list = listing(f103);
      assert.equal(list.catalogue.cpu, 'test-f103');
      assert.equal(list.catalogue.registers.length, 330);
      assert.equal(hash(index), indexHash);
      return {definitions:330, note:'Test-only inherited M3 catalogue; no F103 hardware validation'};
    });
    await suite.test('HOST-MISSING-DEFAULT', 'An uninstalled optional register catalogue falls back with an explanation', async () => {
      fs.unlinkSync(registers);
      const list = listing(f103);
      assert.equal(list.source, 'gdb');
      assert(list.configuration.settings.some(s => s.source.includes('not installed; GDB')));
      assert.equal(hash(index), indexHash);
    });
    await suite.test('HOST-BAD-DESCRIPTION', 'Malformed installed and missing explicit catalogues remain actionable errors', async () => {
      fs.writeFileSync(registers, 'broken=[');
      assert(listing(f103, false).includes('Register catalogue'));
      assert(listing(generic + "[registers]\ncatalogue='missing.toml'\n", false).includes('missing.toml'));
      fs.unlinkSync(registers);
      fs.unlinkSync(chip);
      assert.equal(listing(generic).source, 'gdb');
    });
    await suite.test('HOST-GDB-REGISTERS', 'The generic CLI actually discovers and reads PC/SP/xPSR using GDB/MI', async () => {
      const transcript = path.join(out, 'generic-mi.txt');
      fs.writeFileSync(project, generic + `[gdb]\nexecutable=${quote(process.execPath)}\nargs=[${quote(path.join(root,'tests/mock-gdb.cjs'))}]\ninit=[]\n[gdb.env]\nDEBUGTUI_TEST_REGISTERS='["r0","","sp","pc","xpsr"]'\nDEBUGTUI_TEST_TRANSCRIPT=${quote(transcript)}\n`);
      const session = new Session(binary, project, out);
      try {
        await session.command('connect');
        const status = await session.command('status');
        assert.deepEqual(status.registers.map(r => r.name), ['r0','sp','pc','xpsr']);
        assert(status.registers.every(r => r.value === '0x100000008'));
        const mi = fs.readFileSync(transcript, 'utf8');
        assert(mi.includes('-data-list-register-names'));
        assert(mi.includes('-data-list-register-values x 0 2 3 4'));
        assert(!mi.includes('-data-read-memory-bytes'));
        assert.equal(hash(index), indexHash);
        return {names:status.registers.map(r => r.name), backend:'strict MI fixture'};
      } finally { await session.close(); }
    });
  } finally {
    if (previous === undefined) delete process.env.DEBUGTUI_CONFIG_DIR;
    else process.env.DEBUGTUI_CONFIG_DIR = previous;
    suite.finish();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
