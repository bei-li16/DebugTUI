// Strict read-only Watch fixture. Mutations happen before the matching MI
// response, so race tests never rely on host scheduling or sleeps.
const fs = require('node:fs');
const env = process.env, objects = new Set();
let calls = env.DEBUGTUI_TEST_WATCH_CALLS_OFF ? 'off' : 'on', serial = 0;
const policy = () => { if (env.DEBUGTUI_TEST_WATCH_POLICY_FILE) fs.writeFileSync(env.DEBUGTUI_TEST_WATCH_POLICY_FILE, calls); };
module.exports.initialize = policy;
function changeContext() {
  if (!env.DEBUGTUI_TEST_WATCH_CHANGE) return;
  const context = JSON.parse(fs.readFileSync(env.DEBUGTUI_TEST_CONTEXT_FILE, 'utf8'));
  if (env.DEBUGTUI_TEST_WATCH_CHANGE === 'thread') context.thread = '2';
  if (env.DEBUGTUI_TEST_WATCH_CHANGE === 'frame') context.frame = 1;
  if (env.DEBUGTUI_TEST_WATCH_CHANGE === 'pc') context.pc = '0x10000000c';
  fs.writeFileSync(env.DEBUGTUI_TEST_CONTEXT_FILE, JSON.stringify(context));
}
module.exports.handle = (cmd, {done, error, running, stopped}) => {
  if (cmd === '-gdb-show may-call-functions') { done(`value="${calls}"`); return true; }
  if (cmd === '-gdb-set may-call-functions off') { calls = 'off'; policy(); done(); return true; }
  if (cmd === '-gdb-set may-call-functions on') {
    if (env.DEBUGTUI_TEST_WATCH_RESTORE_ERROR) error('Fixture policy restoration failed');
    else { calls = 'on'; policy(); done(); }
    return true;
  }
  if (cmd === '-var-create - * "counter"') {
    if (calls !== 'off') { error('Target calls must be disabled before Watch creation'); return true; }
    const name = 'watch' + (++serial); objects.add(name);
    done(`name="${name}",numchild="${env.DEBUGTUI_TEST_WATCH_AGGREGATE ? '2' : '0'}",type="unsigned int",value="17"`);
    return true;
  }
  let match = /^-var-(info-path-expression|delete) "(watch\d+)"$/.exec(cmd);
  if (match) {
    if (!objects.has(match[2]) || calls !== 'off') { error('Unbound Watch or calls enabled'); return true; }
    if (match[1] === 'info-path-expression') {
      done(`path_expr=${JSON.stringify(env.DEBUGTUI_TEST_WATCH_PATH || 'counter')}`);
    } else {
      if (env.DEBUGTUI_TEST_WATCH_CHANGE_AT === 'cleanup') changeContext();
      if (env.DEBUGTUI_TEST_WATCH_CLEANUP_ERROR) error('Fixture Watch cleanup failed');
      else { objects.delete(match[2]); done(`ndeleted="${env.DEBUGTUI_TEST_WATCH_ZERO_DELETED ? '0' : '1'}"`); }
    }
    return true;
  }
  if (cmd === '-data-evaluate-expression "(unsigned long long)&(counter)"') {
    if (calls !== 'off') error('Target calls enabled during address lookup');
    else if (env.DEBUGTUI_TEST_WATCH_NO_ADDRESS) error('Value is a register or bitfield without an address');
    else {
      if (env.DEBUGTUI_TEST_WATCH_CHANGE_AT !== 'cleanup') changeContext();
      if (env.DEBUGTUI_TEST_WATCH_RUNNING) running();
      if (env.DEBUGTUI_TEST_WATCH_STOP_AGAIN) stopped();
      if (env.DEBUGTUI_TEST_WATCH_THREAD_NOTICE) process.stdout.write('=thread-selected,id="2",frame={level="0",addr="0x100000008"}\n');
      if (env.DEBUGTUI_TEST_WATCH_READY_FILE) {
        fs.writeFileSync(env.DEBUGTUI_TEST_WATCH_READY_FILE, 'address dispatched');
        const timer = setInterval(() => {
          if (fs.existsSync(env.DEBUGTUI_TEST_WATCH_RELEASE_FILE)) {
            clearInterval(timer); done('value="0x100000004"');
          }
        }, 5);
      } else done('value="0x100000004"');
    }
    return true;
  }
  if (cmd === '-data-evaluate-expression "sizeof(counter)"') { done('value="4"'); return true; }
  if (cmd === '-data-evaluate-expression "(__typeof__(counter))1.5"') { done('value="1"'); return true; }
  if (cmd === '-data-evaluate-expression "(__typeof__(counter))-1"') { done('value="4294967295"'); return true; }
  return false;
};
