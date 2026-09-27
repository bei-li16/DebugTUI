// Exercise the shipped CLI process, not just Options::parse.
const {spawnSync} = require('node:child_process');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, quote, hash, outputDirectory, parseOptions, Cases} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary']);
const binary = path.resolve(options.binary || path.join(root, 'target/debug/debugtui.exe'));
const out = outputDirectory('cli');
const suite = new Cases(out, {binary, sha256: hash(binary), layer: 'cli-process'});
const invoke = (args, input) => {
  const result = spawnSync(binary, args, {cwd: out, input, encoding: 'utf8', windowsHide: true, timeout: 20000});
  assert.ifError(result.error);
  return result;
};
const decode = result => result.stdout.trim().split(/\r?\n/).filter(Boolean).map(JSON.parse);
const project = path.join(out, '工程 with spaces.toml');
fs.writeFileSync(project, `version=2\nwatch=[]\n[gdb]\nexecutable=${quote(process.execPath)}\nargs=[${quote(path.join(root, 'tests/mock-gdb.cjs'))}]\n[gdb.env]\nDEBUGTUI_TEST_REGISTERS='["pc","sp"]'\nDEBUGTUI_TEST_TRANSCRIPT=${quote(path.join(out, 'fixture.mi.txt'))}\n[target]\nmode='remote'\nendpoint='localhost:1234'\n[session]\non_exit='detach'\n`);
const runScript = (name, text, file = project) => {
  const script = path.join(out, `${name}.jsonl`);
  fs.writeFileSync(script, text);
  const result = invoke(['--project', file, '--script', script]);
  fs.writeFileSync(path.join(out, `${name}.stdout.jsonl`), result.stdout);
  fs.writeFileSync(path.join(out, `${name}.stderr.txt`), result.stderr);
  return {result, events: decode(result)};
};
(async () => {
  await suite.test('CLI-01', 'Version matches Cargo/package metadata and aliases', () => {
    const version = JSON.parse(fs.readFileSync(path.join(root, 'package.json'))).version;
    for (const flag of ['--version', '-V']) { const result = invoke([flag]); assert.equal(result.status, 0); assert.equal(result.stdout.trim(), `debugtui ${version}`); }
  });
  await suite.test('CLI-02', 'Help describes launch, environment, script and SVD options', () => {
    for (const flag of ['--help', '-h']) { const result = invoke([flag]); assert.equal(result.status, 0); for (const option of ['--setup', '--project', '--environment', '--headless', '--script', '--svd']) assert(result.stdout.includes(option)); }
  });
  await suite.test('CLI-03', 'Unknown/removed options fail with an actionable diagnostic', () => {
    for (const flag of ['--not-a-debugtui-option', '--device']) { const result = invoke([flag]); assert.equal(result.status, 1); assert.match(result.stderr, /Unknown option.*Use --help/s); }
  });
  await suite.test('CLI-04', 'Every value-taking option rejects a missing value', () => {
    for (const option of ['--project','--tools-dir','--environment','--elf','--gdb','--gdb-arg','--connect','--target-mode','--log-dir','--script','--snapshot','--svd']) {
      const result = invoke([option]); assert.equal(result.status, 1); assert(result.stderr.includes(`${option} requires a value`));
    }
  });
  await suite.test('CLI-05', 'Snapshot renderer writes UTF-8 to a Unicode/spaced path', () => {
    const file = path.join(out, '界面 snapshot.txt');
    const result = invoke(['--snapshot', file]); assert.equal(result.status, 0);
    const text = fs.readFileSync(file, 'utf8'); for (const label of ['DebugTUI', 'Watch', 'Source']) assert(text.includes(label));
  });
  await suite.test('CLI-06', 'BOM/CRLF/blank script lines, nonsequential IDs and project paths', () => {
    const {result, events} = runScript('bom', '\ufeff\r\n{"id":41,"method":"connect"}\r\n\r\n{"id":73,"method":"status"}\r\n');
    assert.equal(result.status, 0);
    const responses = events.filter(e => e.event === 'response');
    assert.deepEqual(responses.slice(0, 2).map(e => e.id), [41, 73]);
    assert(responses.every(e => e.ok)); assert(events.some(e => e.event === 'exit'));
  });
  await suite.test('CLI-07', 'Failed request stops scripted execution and performs cleanup', () => {
    const {result, events} = runScript('failure', '{"id":1,"method":"connect"}\n{"id":2,"method":"missing_method"}\n{"id":3,"method":"continue"}\n');
    assert.equal(result.status, 1);
    assert(events.some(e => e.event === 'response' && e.id === 2 && !e.ok));
    assert(!events.some(e => e.event === 'response' && e.id === 3));
    assert(events.some(e => e.event === 'exit'));
    assert(events.some(e => e.event === 'response' && e.id > 1000 && e.ok));
  });
  await suite.test('CLI-08', 'Malformed JSON reports its source line and never dispatches later work', () => {
    const {result, events} = runScript('invalid-json', '{"id":1,"method":"status"}\n{broken}\n{"id":3,"method":"connect"}\n');
    assert.equal(result.status, 1); assert.match(result.stderr, /Line 2/);
    assert(!events.some(e => e.event === 'response' && e.id === 3)); assert(events.some(e => e.event === 'exit'));
  });
  await suite.test('CLI-09', 'Empty script exits cleanly without starting GDB', () => {
    const {result, events} = runScript('empty', '\ufeff\n\n'); assert.equal(result.status, 0);
    assert(!events.some(e => e.event === 'log' && e.channel === 'mi>'));
    assert(events.some(e => e.event === 'exit'));
  });
  await suite.test('CLI-10', 'Headless stdin continues after malformed input and ends cleanly at EOF', () => {
    const result = invoke(['--project', project, '--headless', '--stdio'], '{broken}\n{"id":81,"method":"status"}\n');
    assert.equal(result.status, 0); assert(result.stderr.length > 0);
    assert(decode(result).some(e => e.event === 'response' && e.id === 81 && e.ok));
    assert(decode(result).some(e => e.event === 'exit'));
  });
  await suite.test('CLI-11', 'Legacy server section, future config versions and bad TOML fail before connecting', () => {
    for (const [name, text, expected] of [['legacy', '[server]\nport=3333\n', /Legacy \[server\]/], ['future', 'version=99\n', /Unsupported project format/], ['syntax', '[program\n', /TOML|expected|invalid/i]]) {
      const file = path.join(out, `${name}.toml`); fs.writeFileSync(file, text);
      const result = invoke(['--project', file, '--headless'], ''); assert.equal(result.status, 1); assert.match(result.stderr, expected);
    }
  });
  await suite.test('CLI-12', 'Missing environment diagnostics and snapshot write failures are nonzero', () => {
    for (const args of [['--project', project, '--environment', 'missing-profile.toml', '--headless'], ['--snapshot', path.join(out, 'missing-directory/snapshot.txt')]]) {
      const result = invoke(args, ''); assert.equal(result.status, 1); assert.match(result.stderr, /debugtui:/);
    }
  });
  suite.finish();
})().catch(error => { console.error(error); process.exitCode = 1; });
