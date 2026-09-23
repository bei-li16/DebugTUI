// Deterministic MI lifecycle regression; no hardware or runtime Node dependency.
const {spawnSync} = require('node:child_process');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const root = path.dirname(__dirname);
const binary = path.resolve(process.argv[2] || path.join(root, 'target/release/debugtui.exe'));
const output = path.join(root, 'artifacts', `exit-policy-${Date.now()}`);
fs.mkdirSync(output, {recursive: true});
const q = s => JSON.stringify(s.replaceAll('\\', '/'));
const report = [];
function scenario(mode, policy, initial, failure = '') {
  const name = [mode, policy, initial, failure].filter(Boolean).join('-');
  const transcript = path.join(output, `${name}.mi.txt`);
  const config = path.join(output, `${name}.toml`);
  fs.writeFileSync(config, `version=2\n[gdb]\nexecutable=${q(process.execPath)}\nargs=[${q(path.join(root, 'tests/mock-gdb.cjs'))}]\n[gdb.env]\nDEBUGTUI_TEST_REGISTERS='["pc","sp"]'\nDEBUGTUI_TEST_TRANSCRIPT=${q(transcript)}\nDEBUGTUI_TEST_PAUSE="missing"\nDEBUGTUI_TEST_EXIT_FAILURE=${q(failure)}\n[target]\nmode=${q(mode)}\nendpoint="localhost:1234"\n[session]\non_exit=${q(policy)}\n`);
  const marker = path.join(output, `${name}.exit.txt`);
  // A real GDB may need time after ^exit; the engine must not kill it early.
  const text = fs.readFileSync(config, 'utf8').replace('[target]', `DEBUGTUI_TEST_EXIT_MARKER=${q(marker)}\n[target]`);
  fs.writeFileSync(config, text);
  const requests = [{method: 'connect'}];
  if (mode === 'local' && initial !== 'READY') requests.push({method: 'run'});
  if (initial === 'RUNNING' && mode !== 'local') requests.push({method: 'continue'});
  if (mode === 'local' && initial === 'STOPPED') requests.push({method: 'pause'});
  requests.push({method: 'status'}, {method: 'disconnect'}, {method: 'status'}, {method: 'quit'});
  requests.forEach((r, i) => r.id = i + 1);
  const script = path.join(output, `${name}.jsonl`);
  fs.writeFileSync(script, requests.map(JSON.stringify).join('\n'));
  const result = spawnSync(binary, ['--project', config, '--script', script], {encoding: 'utf8', timeout: 30000, windowsHide: true});
  fs.writeFileSync(path.join(output, `${name}.events.jsonl`), result.stdout || '');
  fs.writeFileSync(path.join(output, `${name}.stderr.txt`), result.stderr || '');
  assert.ifError(result.error);
  const responses = result.stdout.trim().split(/\r?\n/).map(JSON.parse).filter(e => e.event === 'response');
  const response = r => responses.find(e => e.id === r.id);
  const statuses = requests.filter(r => r.method === 'status').map(response);
  assert.equal(statuses[0].result.state, initial, name);
  const disconnect = response(requests.find(r => r.method === 'disconnect'));
  assert.equal(disconnect.ok, !failure, `${name}: ${disconnect.error}`);
  if (failure) {
    // Script mode stops on the first failed request and then cleans up.
    const events = result.stdout.trim().split(/\r?\n/).map(JSON.parse);
    assert(events.some(e => e.event === 'snapshot' && e.snapshot.state === 'FAULT'), name);
  } else assert.equal(statuses[1].result.state, 'DISCONNECTED', name);
  if (failure) assert.match(disconnect.error, failure === 'hang' ? /GDB did not exit after \^exit/ : failure === 'crash' ? /GDB exited unsuccessfully/ : new RegExp(`Fixture ${failure} failed`));
  else assert(responses.every(r => r.ok), name);
  assert.equal(result.status, failure ? 1 : 0, name);
  const mi = fs.readFileSync(transcript, 'utf8').trim().split(/\r?\n/);
  const cleanupStart = mi.indexOf('-interpreter-exec console "delete breakpoints"');
  const exit = mi.slice(cleanupStart < 0 ? 0 : cleanupStart);
  const remoteResume = policy === 'resume' && mode !== 'local';
  const release = policy === 'disconnect' || remoteResume ? '-target-disconnect' : '-target-detach';
  if (initial === 'READY') {
    assert(!exit.includes('-target-detach') && !exit.includes('-target-disconnect'), name);
  } else {
    assert(cleanupStart >= 0, `${name}: breakpoints not cleared`);
    assert(exit.includes(release), `${name}: missing ${release}`);
    assert.equal(exit.includes('-exec-continue'), remoteResume, `${name}: wrong resume sequence`);
    if (remoteResume) assert(exit.indexOf('-exec-continue') < exit.indexOf(release), name);
    assert(exit.indexOf(release) < exit.indexOf('-gdb-exit'), name);
  }
  assert.equal(mi.filter(c => c === '-gdb-exit').length, 1, `${name}: duplicate shutdown`);
  if (!['exit', 'hang', 'crash'].includes(failure)) assert.equal(fs.readFileSync(marker, 'utf8'), 'clean exit', name);
  report.push({name, passed: true, commands: exit});
  console.log(`PASS ${name}`);
}
for (const mode of ['remote', 'extended-remote', 'local'])
  for (const policy of ['resume', 'detach', 'disconnect'])
    for (const initial of ['STOPPED', 'RUNNING']) scenario(mode, policy, initial);
for (const policy of ['resume', 'detach', 'disconnect']) scenario('local', policy, 'READY');
for (const failure of ['continue', 'release', 'exit', 'hang', 'crash']) scenario('remote', 'resume', 'STOPPED', failure);
fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
console.log(`PASS ${report.length} exit-policy scenarios: ${output}`);
