#!/usr/bin/env node
'use strict';
// Release readiness only: offline EXE configuration and default deferred drivers.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {root, outputDirectory, parseOptions, Cases, hash} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary']);
const binary = path.resolve(options.binary || path.join(root, 'target/debug/debugtui.exe'));
const out = outputDirectory('register-readonly-readiness');
const suite = new Cases(out, {layer: 'actual EXE offline templates and guarded default deferred drivers',
  board_tests_executed: false, binary, binary_sha256: hash(binary)});

function template(name, cores) {
  const directory = path.join(out, name), config = path.join(directory, 'config');
  fs.mkdirSync(config, {recursive: true});
  const source = path.join(root, 'profiles', `${name}.toml.example`);
  const project = path.join(directory, 'debug.toml');
  fs.copyFileSync(source, project);
  const before = hash(project);
  const requests = cores.flatMap(name => [['select_core', {name}], ['registers_list', {}],
    ['registers_matrix', {}], ['status', {}]]).concat([['quit', {}]]);
  const result = spawnSync(binary, ['--project', project, '--headless', '--stdio'], {
    input: requests.map(([method, params], i) => JSON.stringify({id: i + 1, method, params})).join('\n') + '\n',
    encoding: 'utf8', windowsHide: true, timeout: 30000, maxBuffer: 32 * 1024 * 1024,
    env: {...process.env, DEBUGTUI_CONFIG_DIR: config, PATH: config},
  });
  fs.writeFileSync(path.join(directory, 'stdout.jsonl'), result.stdout || '');
  fs.writeFileSync(path.join(directory, 'stderr.log'), result.stderr || '');
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  assert.equal(hash(project), before, 'offline inspection must preserve the project');
  assert.equal(hash(source), before, 'the tested project must be the exact shipped template');
  const events = result.stdout.trim().split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  assert(!events.some(e => e.event === 'log' && e.channel === 'mi>'), 'no debugger traffic');
  const responses = events.filter(e => e.event === 'response');
  assert.equal(responses.length, requests.length);
  for (const response of responses) assert(response.ok, JSON.stringify(response));
  const results = method => responses.filter(e => requests[e.id - 1][0] === method).map(e => e.result);
  const lists = results('registers_list'), matrices = results('registers_matrix');
  for (const [i, matrix] of matrices.entries()) {
    assert.equal(matrix.context.core, cores[i]); assert.equal(lists[i].context.core, cores[i]);
    assert.equal(matrix.probe_current, false); assert.equal(matrix.probe, null);
    assert.deepEqual(matrix.observations, []);
    assert(matrix.rows.every(row => row.support !== 'observed_value'), 'plans cannot claim observed hardware');
  }
  return {source, sha256: before, directory, lists, matrices};
}
function row(matrix, id) {
  const entry = matrix.rows.find(row => row.id === id); assert(entry, `Missing ${id}`); return entry;
}

const drivers = [
  ['test-m-profile-modules-hardware.cjs', 5], ['test-m-profile-mpu-hardware.cjs', 3],
  ['test-m7-cache-hardware.cjs', 4], ['test-m-profile-multicore-hardware.cjs', 4],
  ['test-register-running-hardware.cjs', 4], ['test-register-r52-core-hardware.cjs', 4],
  ['test-mpu-regions-hardware.cjs', 4], ['test-register-selectors-hardware.cjs', 4],
];
// A default driver may write reports, but may not launch anything or open sockets.
// The guard is generated outside the repository and loaded before each driver.
const guard = path.join(out, 'deny-target-io.cjs');
fs.writeFileSync(guard, `const fs=require('node:fs');
function deny(api){return function(){fs.appendFileSync(process.env.DEBUGTUI_READINESS_IO_LOG,api+'\\n');throw Error('Default hardware driver attempted '+api);};}
const child=require('node:child_process');
for(const name of ['spawn','spawnSync','exec','execSync','execFile','execFileSync','fork'])child[name]=deny('child_process.'+name);
const net=require('node:net');net.Socket.prototype.connect=deny('net.Socket.connect');
for(const name of ['connect','createConnection'])net[name]=deny('net.'+name);
`);

(async () => {
  try {
    await suite.test('READONLY-TEMPLATE-M', 'Actual EXE keeps M7/M4 catalogue, PPB route and owner distinct', async () => {
      const result = template('registers-readonly-multicore', ['m7', 'm4']);
      for (const [i, matrix] of result.matrices.entries()) {
        const core = i ? 'm4' : 'm7', cpuid = row(matrix, 'scb.cpuid');
        assert.equal(result.lists[i].catalogue.cpu, `cortex-${core}`);
        assert.equal(cpuid.owner, `core:${core}`); assert.equal(cpuid.plan.available, true);
        assert.equal(cpuid.plan.channel, `${core}_ppb`); assert.equal(cpuid.plan.target, `example.${core}`);
        assert.equal(cpuid.plan.endpoint, '127.0.0.1:6666'); assert.equal(cpuid.plan.address, '0xe000ed00');
      }
      assert(result.matrices[0].rows.some(row => row.id === 'scb.clidr'));
      assert(!result.matrices[1].rows.some(row => row.id === 'scb.clidr'));
      return {source: result.source, sha256: result.sha256, directory: result.directory,
        routes: result.matrices.map(matrix => row(matrix, 'scb.cpuid'))};
    });
    await suite.test('READONLY-TEMPLATE-R52', 'Actual EXE inherits native R52 readers and distinct physical targets', async () => {
      const result = template('registers-readonly-r52', ['core0', 'core1']);
      for (const [i, matrix] of result.matrices.entries()) {
        assert.equal(result.lists[i].catalogue.cpu, 'cortex-r52');
        for (const id of ['midr', 'mpuir', 'hmpuir', 'prbar0']) {
          const entry = row(matrix, id);
          assert.equal(entry.owner, `core:core${i}`); assert.equal(entry.plan.available, true);
          assert.equal(entry.plan.operation, `aarch64 r52_read ${id}`);
          assert.equal(entry.plan.target, `example.r52.${i}`); assert.equal(entry.plan.endpoint, '127.0.0.1:6666');
          assert(entry.plan.protocol.startsWith('debugtui-r52-core-1 '));
        }
        const settings = result.lists[i].configuration.settings;
        assert.equal(settings.find(s => JSON.stringify(s.path) === '["selector_command"]').value, 'aarch64 r52_select');
        assert.equal(settings.find(s => JSON.stringify(s.path) === '["isb_command"]').value, '');
      }
      return {source: result.source, sha256: result.sha256, directory: result.directory,
        routes: result.matrices.map(matrix => row(matrix, 'midr'))};
    });
    for (const [name, count] of drivers) {
      await suite.test(`READONLY-DEFERRED-${name}`, 'Default driver reports SKIPPED with guarded zero target I/O', async () => {
        const directory = path.join(out, name); fs.mkdirSync(directory);
        const ioLog = path.join(directory, 'target-io.log'); fs.writeFileSync(ioLog, '');
        const source = path.join(root, 'scripts', name);
        const result = spawnSync(process.execPath, ['--require', guard, source], {
          encoding: 'utf8', windowsHide: true, timeout: 15000, maxBuffer: 1024 * 1024,
          env: {...process.env, DEBUGTUI_TEST_ARTIFACT_ROOT: directory, DEBUGTUI_READINESS_IO_LOG: ioLog},
        });
        fs.writeFileSync(path.join(directory, 'stdout.log'), result.stdout || '');
        fs.writeFileSync(path.join(directory, 'stderr.log'), result.stderr || '');
        assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
        assert.equal(fs.readFileSync(ioLog, 'utf8'), '', 'no child process or TCP connection attempted');
        const reports = fs.readdirSync(directory, {withFileTypes: true}).filter(e => e.isDirectory())
          .map(e => path.join(directory, e.name, 'report.json')).filter(file => fs.existsSync(file));
        assert.equal(reports.length, 1);
        const report = JSON.parse(fs.readFileSync(reports[0], 'utf8'));
        assert.equal(report.board_tests_executed, false); assert.equal(report.passed, false);
        assert.deepEqual(report.counts, {passed: 0, failed: 0, skipped: count});
        assert(report.cases.every(item => item.status === 'skipped' && item.reason));
        return {source, sha256: hash(source), report: reports[0], counts: report.counts, target_io_attempts: 0};
      });
    }
  } catch (error) {
    await suite.test('READONLY-READINESS-SETUP', 'Finish software readiness checks', async () => {throw error;});
  } finally {
    suite.finish();
  }
})();
