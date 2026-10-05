// Development-only helpers. Never included in the installed debugger.
const {spawn, spawnSync} = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../..');
const quote = value => JSON.stringify(value.replaceAll('\\', '/'));
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');

function outputDirectory(name, artifactRoot = path.join(root, 'artifacts')) {
  const directory = path.join(path.resolve(artifactRoot), `${name}-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`);
  fs.mkdirSync(directory, {recursive: true});
  return directory;
}

function parseOptions(args, valueOptions, flagOptions = []) {
  const result = {};
  for (let i = 0; i < args.length; i++) {
    const key = args[i];
    assert(key.startsWith('--'), `Expected an option, got ${key}`);
    if (flagOptions.includes(key)) result[key.slice(2)] = true;
    else {
      assert(valueOptions.includes(key), `Unknown option: ${key}`);
      assert(args[i + 1] && !args[i + 1].startsWith('--'), `${key} requires a value`);
      result[key.slice(2)] = args[++i];
    }
  }
  return result;
}

class Cases {
  constructor(directory, metadata = {}) { this.directory = directory; this.metadata = metadata; this.results = []; }
  async test(id, description, body) {
    const start = Date.now();
    try {
      const evidence = await body();
      this.results.push({id, description, status: 'passed', duration_ms: Date.now() - start, evidence});
      console.log(`PASS ${id}: ${description}`);
      return true;
    } catch (error) {
      this.results.push({id, description, status: 'failed', duration_ms: Date.now() - start, error: error.stack || String(error)});
      console.error(`FAIL ${id}: ${error.message}`);
      return false;
    }
  }
  skip(id, description, reason) { this.results.push({id, description, status: 'skipped', reason}); }
  finish() {
    const counts = {passed: 0, failed: 0, skipped: 0};
    for (const item of this.results) counts[item.status]++;
    const report = {...this.metadata, counts, passed: counts.failed === 0 && counts.skipped === 0, cases: this.results};
    fs.writeFileSync(path.join(this.directory, 'report.json'), JSON.stringify(report, null, 2));
    console.log(`RESULT ${JSON.stringify(counts)} ${this.directory}`);
    if (counts.failed) process.exitCode = 1;
    return report;
  }
}

class Session {
  constructor(binary, project, directory, extra = []) {
    this.events = [];
    this.pending = new Map();
    this.nextId = 0;
    this.child = spawn(binary, ['--project', project, '--log-dir', directory, '--headless', '--stdio', ...extra], {windowsHide: true});
    this.finished = false;
    this.child.stdin.on('error', error => this.failPending(error));
    this.exit = new Promise(resolve => {
      this.child.once('error', error => { this.finished = true; this.failPending(error); resolve({code: null, error: error.message}); });
      this.child.once('exit', () => { this.finished = true; });
      // exit can precede buffered stdout. Drain the response pipe before
      // rejecting a pending quit or closing its evidence file.
      this.child.once('close', (code, signal) => { this.finished = true; this.failPending(Error(`Debugger exited (${code}, ${signal})`)); resolve({code, signal}); });
    });
    this.stream = fs.createWriteStream(path.join(directory, `events-${crypto.randomUUID().slice(0, 8)}.jsonl`));
    readline.createInterface({input: this.child.stdout}).on('line', line => {
      this.stream.write(line + '\n');
      try {
        const event = JSON.parse(line);
        this.events.push(event);
        if (event.event === 'response') this.pending.get(event.id)?.resolve(event);
      } catch (error) { this.protocolError = Error(`Invalid debugger JSON: ${error.message}`); this.failPending(this.protocolError); }
    });
    this.child.stderr.on('data', data => fs.appendFileSync(path.join(directory, 'stderr.log'), data));
  }
  failPending(error) { for (const entry of this.pending.values()) entry.reject(error); }
  async command(method, params = {}, expectedOk = true, timeout = 30000) {
    assert(!this.finished, 'Debugger already exited');
    if (this.protocolError) throw this.protocolError;
    const id = ++this.nextId;
    const response = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.pending.get(id)?.reject(Error(`Timeout: ${method}`)), timeout);
      const finish = callback => value => { clearTimeout(timer); this.pending.delete(id); callback(value); };
      this.pending.set(id, {resolve: finish(resolve), reject: finish(reject)});
      this.child.stdin.write(JSON.stringify({id, method, params}) + '\n', error => { if (error) this.pending.get(id)?.reject(error); });
    });
    assert.equal(response.ok, expectedOk, `${method}: ${response.error}`);
    return expectedOk ? response.result : response.error;
  }
  logs(channel) { return this.events.filter(e => e.event === 'log' && (!channel || e.channel === channel)); }
  async close() {
    let failure;
    try { if (!this.finished) await this.command('quit', {}, true, 15000); }
    catch (error) { failure = error; }
    this.child.stdin.end();
    let timer;
    const result = await Promise.race([this.exit, new Promise(resolve => { timer = setTimeout(() => resolve(null), 15000); })]);
    clearTimeout(timer);
    if (!result) {
      // Kill only the process tree launched by this test, never unrelated GDBs.
      if (process.platform === 'win32') spawnSync('taskkill.exe', ['/PID', String(this.child.pid), '/T', '/F'], {windowsHide: true, timeout: 10000});
      else this.child.kill('SIGKILL');
      failure ??= Error('Debugger cleanup timed out');
    } else if (result.code !== 0) failure ??= Error(`Debugger exit: ${JSON.stringify(result)}`);
    await new Promise(resolve => this.stream.end(resolve));
    if (failure) throw failure;
  }
}

module.exports = {root, quote, delay, hash, outputDirectory, parseOptions, Cases, Session};
