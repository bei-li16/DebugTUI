// Strict MI fixture used only in development tests; never shipped with the TUI.
const fs = require('node:fs');
const readline = require('node:readline');
const names = JSON.parse(process.env.DEBUGTUI_TEST_REGISTERS);
const rawValues = JSON.parse(process.env.DEBUGTUI_TEST_REGISTER_VALUES || '{}');
const unreadableRegisters = JSON.parse(process.env.DEBUGTUI_TEST_REGISTER_ERRORS || '[]');
const transcript = process.env.DEBUGTUI_TEST_TRANSCRIPT;
let state = 'ready';
let line = 10;
const pauseMode = process.env.DEBUGTUI_TEST_PAUSE;
const exitFailure = process.env.DEBUGTUI_TEST_EXIT_FAILURE;
let interrupts = 0;
let frameLevel = 0;
let registerWritten = false;
let writerProbed = false;
let registerReadCount = 0;
let memoryWritten = false;
let memoryWriterProbed = false;
const variables = process.env.DEBUGTUI_TEST_VARIABLES ? require('./mock-variable-gdb.cjs') : null;
const memory = new Map();
const memoryBase = BigInt(process.env.DEBUGTUI_TEST_RAM_BASE || '0x20000000');
for (let i = 0; i < 4098; i++) memory.set(memoryBase + BigInt(i), 0xaa);
variables?.initialize(memory);
const frame = () => `frame={level="${frameLevel}",addr="0x100000008",func="main",file="sample.c",line="${line}"}`;
const send = value => process.stdout.write(value + '\n');
readline.createInterface({ input: process.stdin }).on('line', input => {
  const match = /^(\d+)(.*)$/.exec(input);
  if (!match) return;
  const [, token, cmd] = match;
  fs.appendFileSync(transcript, cmd + '\n');
  const done = data => send(`${token}^done${data ? ',' + data : ''}`);
  if (variables?.handle(cmd, {done, memory, error: message => send(`${token}^error,msg=${JSON.stringify(message)}`), running: () => {state='running';send('*running,thread-id="all"');}, stopped: () => {state='stopped';send(`*stopped,reason="signal-received",${frame()}`);}})) return;
  if (cmd.startsWith('-gdb-set ') || cmd.startsWith('-file-exec-and-symbols ')) return done();
  if (cmd === '-gdb-show may-write-registers') {
    const disabled = process.env.DEBUGTUI_TEST_REGISTER_READONLY || (writerProbed && process.env.DEBUGTUI_TEST_WRITE_PERMISSION_CHANGE);
    return done(`value="${disabled ? 'off' : 'on'}"`);
  }
  if (cmd === '-gdb-show may-write-memory') return done(`value="${process.env.DEBUGTUI_TEST_MEMORY_READONLY || (memoryWriterProbed && process.env.DEBUGTUI_TEST_MEMORY_PERMISSION_CHANGE) ? 'off' : 'on'}"`);
  if (cmd === '-info-gdb-mi-command data-write-memory-bytes') {
    memoryWriterProbed = true;
    return done(`command={exists="${process.env.DEBUGTUI_TEST_NO_MEMORY_WRITER ? 'false' : 'true'}"}`);
  }
  if (cmd.startsWith('-data-write-memory-bytes ')) {
    const write = /^-data-write-memory-bytes (0x[0-9a-f]+) ([0-9a-f]+)$/.exec(cmd);
    if (!write || write[2].length % 2 || write[2].length > 8192) return send(`${token}^error,msg="Invalid fixture memory write"`);
    const base = BigInt(write[1]);
    memoryWritten = true;
    for (let i=0;i<write[2].length/2;i++) memory.set(base+BigInt(i), process.env.DEBUGTUI_TEST_MEMORY_WRITE_MISMATCH ? 0 : parseInt(write[2].slice(i*2,i*2+2),16));
    const failure = process.env.DEBUGTUI_TEST_MEMORY_WRITE_ERROR;
    if (failure === 'closed') {process.exit(7);return;}
    if (failure === 'timeout') return;
    if (failure === 'error') return send(`${token}^error,msg="Memory error after target write"`);
    if (process.env.DEBUGTUI_TEST_MEMORY_WRITE_RUN) {state='running';send('*running,thread-id="all"');}
    return done();
  }
  if (cmd.startsWith('-data-read-memory-bytes ') && process.env.DEBUGTUI_TEST_RAM) {
    if (memoryWritten && process.env.DEBUGTUI_TEST_MEMORY_VERIFY_ERROR) return send(`${token}^error,msg="Memory readback unavailable"`);
    const read=/^-data-read-memory-bytes "(0x[0-9a-f]+)" (\d+)$/.exec(cmd);
    if (!read) return send(`${token}^error,msg="Invalid fixture memory read"`);
    const base=BigInt(read[1]), count=Number(read[2]);
    if (!count || count>4096) return send(`${token}^error,msg="Invalid fixture byte count"`);
    const contents=Array.from({length:count},(_,i)=>(memory.get(base+BigInt(i)) ?? 0xaa).toString(16).padStart(2,'0')).join('');
    return done(`memory=[{begin="0x${base.toString(16)}",offset="0x0",end="0x${(base+BigInt(count)).toString(16)}",contents="${contents}"}]`);
  }
  if (cmd.startsWith('-target-select ')) {
    state = 'stopped'; send(`${token}^connected`); return;
  }
  if (cmd === '-list-target-features') return done('features=["async"]');
  if (cmd === '-thread-info') {
    if (state === 'running' && pauseMode === 'query-rejected') return send(`${token}^error,msg="Cannot execute this command while the target is running."`);
    const thread = (writerProbed && process.env.DEBUGTUI_TEST_WRITE_THREAD_CHANGE) || (registerReadCount && process.env.DEBUGTUI_TEST_CAPABILITY_THREAD_CHANGE) ? '2' : '1';
    return done(state === 'ready' ? 'threads=[]' : `threads=[{id="${thread}",state="${state}"}],current-thread-id="${thread}"`);
  }
  if (cmd === '-stack-info-frame') {
    if (registerReadCount && process.env.DEBUGTUI_TEST_CAPABILITY_FRAME_CHANGE) frameLevel = 1;
    return state === 'stopped' ? done(frame()) : send(`${token}^error,msg="No frame"`);
  }
  if (cmd.startsWith('-stack-list-frames')) return done(`stack=[${frame()}]`);
  if (cmd.startsWith('-stack-list-variables')) return done('variables=[{name="counter",value="42"}]');
  if (/^-stack-select-frame \d+$/.test(cmd)) { frameLevel = Number(cmd.split(' ')[1]); return done(); }
  if (cmd === '-info-gdb-mi-command data-write-register-values') {
    writerProbed = true;
    return done(`command={exists="${process.env.DEBUGTUI_TEST_NO_REGISTER_WRITER ? 'false' : 'true'}"}`);
  }
  if (cmd.startsWith('-data-write-register-values ')) {
    const write = /^-data-write-register-values x (\d+) (0x[0-9a-f]+)$/.exec(cmd);
    if (!write || !names[Number(write[1])]) return send(`${token}^error,msg="Invalid fixture write"`);
    registerWritten = true;
    rawValues[names[Number(write[1])]] = process.env.DEBUGTUI_TEST_WRITE_MISMATCH ? '0x00000000' : write[2];
    if (process.env.DEBUGTUI_TEST_WRITE_ERROR === 'closed') { process.exit(7); return; }
    if (process.env.DEBUGTUI_TEST_WRITE_ERROR === 'timeout') return;
    if (process.env.DEBUGTUI_TEST_WRITE_ERROR === 'error') return send(`${token}^error,msg="Fixture error after target write"`);
    if (process.env.DEBUGTUI_TEST_WRITE_RUN) { state = 'running'; send('*running,thread-id="all"'); }
    return done();
  }
  if (cmd === '-data-list-register-names') return done('register-names=' + JSON.stringify(names));
  if (cmd.startsWith('-data-list-register-values x ')) {
    const indices = cmd.slice('-data-list-register-values x '.length).split(' ').map(Number);
    return done('register-values=[' + indices.map(i => `{number="${i}",value="0x100000008"}`).join(',') + ']');
  }
  if (cmd.startsWith('-data-list-register-values r ')) {
    registerReadCount++;
    if (registerWritten && process.env.DEBUGTUI_TEST_WRITE_VERIFY_ERROR) return send(`${token}^error,msg="Fixture readback unavailable"`);
    const indices = cmd.slice('-data-list-register-values r '.length).split(' ').map(Number);
    if (indices.some(index => unreadableRegisters.includes(names[index]))) {
      return send(`${token}^error,msg="Register is inaccessible"`);
    }
    if (process.env.DEBUGTUI_TEST_REGISTER_RUN_ON_READ) {
      state = 'running';
      send('*running,thread-id="all"');
    }
    const overrideFile = process.env.DEBUGTUI_TEST_REGISTER_VALUES_FILE;
    const overrides = overrideFile && fs.existsSync(overrideFile) ? JSON.parse(fs.readFileSync(overrideFile, 'utf8')) : {};
    return done('register-values=[' + indices.map(index => `{number="${index}",value="${overrides[names[index]] || rawValues[names[index]] || '0x12345678'}"}`).join(',') + ']');
  }
  if (cmd === '-break-list') return done('BreakpointTable={body=[]}');
  const evaluateRegister = /^-data-evaluate-expression "\$(r(?:[0-9]|1[0-2]))"$/.exec(cmd);
  if (evaluateRegister && names.includes(evaluateRegister[1])) return done(`value="${rawValues[evaluateRegister[1]] || '0x12345678'}"`);
  if (cmd.startsWith('-data-read-memory-bytes ') && process.env.DEBUGTUI_TEST_MEMORY_BLOCKS) {
    if (process.env.DEBUGTUI_TEST_MEMORY_RUN_ON_READ) {
      state = 'running'; send('*running,thread-id="all"');
    }
    return done('memory=' + process.env.DEBUGTUI_TEST_MEMORY_BLOCKS);
  }
  if (cmd === '-interpreter-exec console "delete breakpoints"') return done();
  // The coordinator refreshes each core after connecting; the fixture has no register cache.
  if (cmd === '-interpreter-exec console "maintenance flush register-cache"') return done();
  if (cmd === '-exec-interrupt --all' && pauseMode) {
    interrupts++;
    if (pauseMode === 'running' || (pauseMode === 'retry' && interrupts === 1)) return done();
    if (pauseMode === 'late' || pauseMode === 'query-rejected') {
      setTimeout(() => { state = 'stopped'; line++; send(`*stopped,reason="signal-received",${frame()}`); }, pauseMode === 'query-rejected' ? 600 : 150);
      return done();
    }
    state = 'stopped';
    line++;
    if (pauseMode === 'already-stopped') return send(`${token}^error,msg="Inferior not executing."`);
    return done(); // Deliberately omit *stopped to exercise state reconciliation.
  }
  if (cmd === '-exec-interrupt --all') {
    state = 'stopped';
    done();
    send(`*stopped,reason="signal-received",${frame()}`);
    return;
  }
  if (cmd === '-target-detach' && state === 'running') return send(`${token}^error,msg="Cannot execute this command while the target is running."`);
  if (cmd === '-target-detach' || cmd === '-target-disconnect') {
    if (exitFailure === 'release') return send(`${token}^error,msg="Fixture release failed"`);
    state = 'ready'; return done();
  }
  if (cmd === '-gdb-exit') {
    if (exitFailure === 'exit') return send(`${token}^error,msg="Fixture exit failed"`);
    if (exitFailure === 'hang') { send(`${token}^exit`); return; }
    if (exitFailure === 'crash') { send(`${token}^exit`); process.exit(7); }
    if (process.env.DEBUGTUI_TEST_EXIT_MARKER) {
      send(`${token}^exit`);
      setTimeout(() => { fs.writeFileSync(process.env.DEBUGTUI_TEST_EXIT_MARKER, 'clean exit'); process.exit(0); }, 100);
      return;
    }
    send(`${token}^exit`); process.exit(0);
  }
  if (/^-exec-(run|continue|step|next|step-instruction|finish)$/.test(cmd)) {
    if (cmd === '-exec-continue' && exitFailure === 'continue') return send(`${token}^error,msg="Fixture continue failed"`);
    state = 'running'; send(`${token}^running`); send('*running,thread-id="all"');
    if (pauseMode) return;
    setTimeout(() => { state = 'stopped'; line++; send(`*stopped,reason="end-stepping-range",${frame()}`); }, 10);
    return;
  }
  send(`${token}^error,msg=${JSON.stringify('Unsupported fixture command: ' + cmd)}`);
});
