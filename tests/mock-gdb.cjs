// Strict MI fixture used only in development tests; never shipped with the TUI.
const fs = require('node:fs');
const readline = require('node:readline');
const names = JSON.parse(process.env.DEBUGTUI_TEST_REGISTERS);
const transcript = process.env.DEBUGTUI_TEST_TRANSCRIPT;
let state = 'ready';
let line = 10;
const pauseMode = process.env.DEBUGTUI_TEST_PAUSE;
let interrupts = 0;
const frame = () => `frame={level="0",addr="0x100000008",func="main",file="sample.c",line="${line}"}`;
const send = value => process.stdout.write(value + '\n');
readline.createInterface({ input: process.stdin }).on('line', input => {
  const match = /^(\d+)(.*)$/.exec(input);
  if (!match) return;
  const [, token, cmd] = match;
  fs.appendFileSync(transcript, cmd + '\n');
  const done = data => send(`${token}^done${data ? ',' + data : ''}`);
  if (cmd.startsWith('-gdb-set ') || cmd.startsWith('-file-exec-and-symbols ')) return done();
  if (cmd.startsWith('-target-select ')) {
    state = 'stopped'; send(`${token}^connected`); return;
  }
  if (cmd === '-list-target-features') return done('features=["async"]');
  if (cmd === '-thread-info') {
    if (state === 'running' && pauseMode === 'query-rejected') return send(`${token}^error,msg="Cannot execute this command while the target is running."`);
    return done(state === 'ready' ? 'threads=[]' : `threads=[{id="1",state="${state}"}],current-thread-id="1"`);
  }
  if (cmd === '-stack-info-frame') return state === 'stopped' ? done(frame()) : send(`${token}^error,msg="No frame"`);
  if (cmd.startsWith('-stack-list-frames')) return done(`stack=[${frame()}]`);
  if (cmd.startsWith('-stack-list-variables')) return done('variables=[{name="counter",value="42"}]');
  if (cmd === '-data-list-register-names') return done('register-names=' + JSON.stringify(names));
  if (cmd.startsWith('-data-list-register-values x ')) {
    const indices = cmd.slice('-data-list-register-values x '.length).split(' ').map(Number);
    return done('register-values=[' + indices.map(i => `{number="${i}",value="0x100000008"}`).join(',') + ']');
  }
  if (cmd === '-break-list') return done('BreakpointTable={body=[]}');
  if (cmd === '-interpreter-exec console "delete breakpoints"') return done();
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
  if (cmd === '-target-detach' || cmd === '-target-disconnect') { state = 'ready'; return done(); }
  if (cmd === '-gdb-exit') { send(`${token}^exit`); process.exit(0); }
  if (/^-exec-(run|continue|step|next|step-instruction|finish)$/.test(cmd)) {
    state = 'running'; send(`${token}^running`); send('*running,thread-id="all"');
    if (pauseMode) return;
    setTimeout(() => { state = 'stopped'; line++; send(`*stopped,reason="end-stepping-range",${frame()}`); }, 10);
    return;
  }
  send(`${token}^error,msg=${JSON.stringify('Unsupported fixture command: ' + cmd)}`);
});
