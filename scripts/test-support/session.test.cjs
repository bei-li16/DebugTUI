// Deterministic child lifecycle order; no debugger, probe or board is started.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {EventEmitter} = require('node:events');
const {PassThrough} = require('node:stream');
const assert = require('node:assert/strict'), test = require('node:test');

function sessionFixture({response = true, ok = true, code = 0} = {}) {
  const child = new EventEmitter();
  for (const name of ['stdin', 'stdout', 'stderr']) child[name] = new PassThrough();
  child.stdin.on('data', data => {
    const request = JSON.parse(data.toString());
    assert.equal(request.method, 'quit');
    // Node's exit notification can precede the final bytes on stdout.
    child.emit('exit', code, null);
    if (response) child.stdout.write(JSON.stringify({event:'response', id:request.id, ok, error:ok?null:'fixture denied quit', result:{closed:true}}) + '\n');
    child.stdout.end();
    child.stderr.end();
    child.emit('close', code, null);
  });
  const module = {exports:{}};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, 'session.cjs'), 'utf8'), {
    module, __dirname, process, setTimeout, clearTimeout,
    require: name => name === 'node:child_process'
      ? {spawn:()=>child, spawnSync:()=>{throw Error('Unexpected forced cleanup');}}
      : require(name),
  }, {filename:path.join(__dirname, 'session.cjs')});
  const directory = module.exports.outputDirectory('session-driver-lifecycle');
  return new module.exports.Session('unused-binary', 'unused-project', directory);
}

test('exit before buffered quit response is drained before cleanup succeeds', async () => {
  const session = sessionFixture();
  await session.close();
  assert.equal(session.events.filter(event=>event.event==='response').length, 1);
  assert.equal(session.pending.size, 0);
});

test('zero exit without a quit response is still a protocol failure', async () => {
  const session = sessionFixture({response:false});
  await assert.rejects(session.close(), /Debugger exited \(0, null\)/);
  assert.equal(session.pending.size, 0);
});

test('a drained error response is not accepted as successful cleanup', async () => {
  const session = sessionFixture({ok:false});
  await assert.rejects(session.close(), /quit: fixture denied quit/);
  assert.equal(session.events[0].ok, false);
});

test('nonzero process exit fails even with a successful quit response', async () => {
  const session = sessionFixture({code:7});
  await assert.rejects(session.close(), /Debugger exit:.*"code":7/);
  assert.equal(session.events[0].ok, true);
});
