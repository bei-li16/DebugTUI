#!/usr/bin/env node
'use strict';
// Parse real OpenOCD configurations and run reset hooks against Tcl mocks.
// Every process exits via shutdown before init: no probe or board is accessed.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {root, outputDirectory, parseOptions, Cases, hash} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--tools']);
const tools = path.resolve(options.tools || path.join(root, 'tools'));
const openocd = path.join(tools, 'bin/openocd/bin/openocd.exe');
const out = outputDirectory('tha-tools');
const suite = new Cases(out, {tools, openocd_sha256:hash(openocd), board_tests_executed:false});
let sequence = 0;
function run(args) {
  const r = spawnSync(openocd, args, {cwd:out, encoding:'utf8', windowsHide:true, timeout:15000});
  fs.writeFileSync(path.join(out, `openocd-${++sequence}.json`), JSON.stringify({args, status:r.status, stdout:r.stdout, stderr:r.stderr}, null, 2));
  assert.ifError(r.error);
  return r;
}
function board(chip, mask, examine, probe='cmsis-dap', extra=[]) {
  return ['-s', path.join(tools, 'bin/openocd/scripts'), '-s', path.join(tools, 'openocd'), '-f', path.join(tools, `openocd/probes/${probe}.cfg`),
    '-c', `set CHIPNAME ${chip}`, '-c', `set DEBUGCORE ${mask}`, '-c', `set EXAMINECORE ${examine}`,
    ...extra, '-f', path.join(tools, 'openocd/tha6.cfg')];
}
function checkBoard(chip, cores, mask, probe='cmsis-dap', extra=[]) {
  const args = board(chip, mask, (1 << cores) - 1, probe, extra);
  for (let core=0; core<cores; core++) args.push(
    '-c', `echo "THA_PORT_${core}=[core.${core} cget -gdb-port]"`,
    '-c', `echo "THA_DEBUG_${core}=[core.${core} cget -dbgbase]"`,
    '-c', `echo "THA_AP_${core}=[core.${core} cget -ap-num]"`
  );
  for (const target of ['APB_1','AHB_3']) args.push('-c', `echo "THA_PORT_${target}=[${target} cget -gdb-port]"`);
  args.push('-c', 'echo "THA_TARGET_COUNT=[llength [target names]]"',
    '-c', 'echo "THA_RESET_HOOKS=[llength [info procs dbgreset]],[llength [info procs chipreset]]"',
    '-c', 'echo "THA_RESET_NOT_RUN=$_dbgreset_done"', '-c', 'shutdown');
  const r = run(args), output = r.stdout + r.stderr;
  assert.equal(r.status, 0, output);
  for (let core=0; core<cores; core++) assert(output.includes(`THA_PORT_${core}=${mask & (1 << core) ? 3333+core : 'disabled'}`), output);
  const debugBases = ['0x80410000','0x80510000','0x80810000','0x80910000'];
  for (let core=0; core<cores; core++) {
    assert(output.includes(`THA_DEBUG_${core}=${debugBases[core]}`), output);
    assert(output.includes(`THA_AP_${core}=1`), output);
  }
  for (const target of ['APB_1','AHB_3']) assert(output.includes(`THA_PORT_${target}=disabled`), output);
  assert(output.includes(`THA_TARGET_COUNT=${cores+2}`), output);
  assert(output.includes('THA_RESET_HOOKS=1,1'), output);
  assert(output.includes('THA_RESET_NOT_RUN=0'), output);
  assert(!output.includes('Listening on port'), 'Offline configuration must not initialize servers');
  return {chip, mask, cores, probe, transport:extra.length ? 'jtag' : 'swd'};
}
(async()=>{
  await suite.test('THA-GDB', 'Bundled GDB accepts ARMv8-R, GHS unwind settings and the readonly Flash window', async()=>{
    const env = {...process.env}; delete env.PYTHONHOME; delete env.PYTHONPATH;
    const commands = ['set architecture armv8-r', 'maintenance set dwarf unwind off', 'set mem inaccessible-by-default off',
      'mem 0x08000000 0x08600000 ro', 'set remotetimeout 10', 'set tcp connect-timeout 5', 'info mem'];
    const r = spawnSync(path.join(tools, 'bin/gdb/bin/arm-none-eabi-gdb.exe'), ['--nx', '--batch',
      '--data-directory='+path.join(tools, 'bin/gdb/arm-none-eabi/share/gdb'), ...commands.flatMap(command=>['-ex',command])],
      {cwd:out, env, encoding:'utf8', windowsHide:true, timeout:15000});
    fs.writeFileSync(path.join(out, 'gdb.json'), JSON.stringify({commands, status:r.status, stdout:r.stdout, stderr:r.stderr}, null, 2));
    assert.ifError(r.error); assert.equal(r.status, 0, r.stdout+r.stderr);
    assert(/0x0?8000000\s+0x0?8600000\s+ro/.test(r.stdout), r.stdout);
  });
  for (const [chip, cores] of [['tha6104',1],['tha6206',2],['tha6412',4]]) {
    for (let mask=1; mask<(1 << cores); mask++) await suite.test(`THA-CFG-${chip}-${mask}`, 'Physical core ports and bus targets parse without hardware', async()=>checkBoard(chip, cores, mask));
  }
  for (const probe of ['jlink','stlink']) await suite.test(`THA-PROBE-${probe}`, 'The THA board shares the independently selected adapter', async()=>checkBoard('tha6412', 4, 10, probe));
  for (const probe of ['cmsis-dap','jlink']) await suite.test(`THA-JTAG-${probe}`, 'JTAG accepts the reference JTAG DPIDR and the same target layout', async()=>checkBoard('tha6412', 4, 5, probe, ['-c','set THA_TRANSPORT jtag']));
  for (const [name, args, expected] of [
    ['unknown-chip', board('tha9999', 1, 1), 'Unsupported THA chip'],
    ['empty-mask', board('tha6206', 0, 3), 'Invalid DEBUGCORE'],
    ['out-of-range', board('tha6104', 2, 3), 'Invalid DEBUGCORE'],
    ['missing-examine', board('tha6206', 2, 1), 'EXAMINECORE must include'],
    ['catalogue-mismatch', board('tha6206', 1, 3, 'cmsis-dap', ['-c','set AVAILABLECORE 15']), 'AVAILABLECORE does not match']
  ]) await suite.test(`THA-REJECT-${name}`, 'Invalid chip/core declarations fail before init', async()=>{
    const r = run([...args, '-c','shutdown']);
    assert.notEqual(r.status, 0);
    assert((r.stdout+r.stderr).includes(expected), r.stdout+r.stderr);
  });
  await suite.test('THA-RESET-ERRORS', 'Reset hooks restore targets, report failures and retry debug recovery', async()=>{
    const file = path.join(out, 'reset-mocks.tcl');
    // Jim Tcl is the same interpreter used by the shipped OpenOCD.
    fs.writeFileSync(file, `
proc check {value message} { if {!$value} { error $message } }
rename targets real_targets
rename target real_target
rename after real_after
foreach name {halt APB_1 AHB_3 core.0 core.1} {
    if {[llength [info commands $name]]} { rename $name real_$name }
}
proc after {ms} {}
set current core.1
set writes {}
set examined {}
set polls {}
set halts {}
set fail ""
proc target {cmd} { global current; return $current }
proc targets {name} { global current; set current $name }
proc halt {timeout} {
    global current halts fail
    check [expr {$timeout == 1000}] "halt must be bounded"
    if {$fail eq "halt"} { error "injected halt timeout" }
    lappend halts $current
}
proc APB_1 {cmd address value} {
    global writes fail
    lappend writes [list $address $value]
    if {$fail eq "application" && $address == 0x80410310} { error "injected reset failure" }
    if {$fail eq "cleanup" && $address == 0x80410024 && $value == 0} { error "injected cleanup failure" }
}
proc AHB_3 {cmd address value} {
    global debug_writes fail
    incr debug_writes
    check [expr {$address == 0xC00008A0 && $value == 1}] "wrong debug reset"
    if {$fail eq "transient" || $fail eq "examine"} { error "transient debug bus failure" }
}
proc cpu {name cmd} {
    global examined polls fail
    if {$cmd eq "arp_examine"} {
        if {$fail eq "examine"} { error "injected examine failure" }
        lappend examined $name
    } elseif {$cmd eq "arp_poll"} { lappend polls $name } else { error "unexpected core operation" }
}
proc core.0 {cmd} { cpu core.0 $cmd }
proc core.1 {cmd} { cpu core.1 $cmd }
set _cores 2
set _examinecore 3
set DBGBASE {0x80410000 0x80510000}
set debug_writes 0
chipreset
check [expr {$current eq "core.1"}] "success changed current target"
check [expr {$examined eq {core.0 core.1} && $polls eq {core.0 core.1} && $halts eq {core.0 core.1}}] "missing recovery/halt"
check [expr {[lindex $writes end] eq {0x80410024 0x0}}] "reset catch was not disabled last"
check [expr {[lindex [lindex $writes 2] 0] == 0x80410090 && [lindex [lindex $writes 2] 1] == 4}] "wrong core0 CSE"
check [expr {[lindex [lindex $writes 3] 0] == 0x80510090 && [lindex [lindex $writes 3] 1] == 4}] "wrong core1 CSE"
chipreset
check [expr {$debug_writes == 1}] "warm reset repeated debug reset"
foreach fail {application halt examine cleanup} {
    set _dbgreset_done 0
    check [catch {chipreset} result] "injected failure was hidden"
    check [expr {[string first "THA chip reset failed" $result] >= 0}] "missing failure context"
    check [expr {$current eq "core.1" && $_dbgreset_done == 0}] "failure did not restore/rearm recovery"
}
set fail transient
chipreset
check [expr {$_dbgreset_done == 1}] "transient write did not recover"
set fail ""
set _examinecore 2
set examined {}
set polls {}
set halts {}
dbgreset
chipreset
check [expr {$examined eq {core.1 core.1} && $polls eq {core.1} && $halts eq {core.1}}] "unexamined core accessed"
echo THA_RESET_MOCKS_PASSED
shutdown
`);
    // Load the complete production board config before mocking its I/O. This
    // verifies the same merged reset procedures shipped to users, without init.
    const r = run([...board('tha6206', 3, 3), '-f', file]);
    assert.equal(r.status, 0, r.stdout+r.stderr);
    assert((r.stdout+r.stderr).includes('THA_RESET_MOCKS_PASSED'));
  });
  suite.finish();
})().catch(error=>{console.error(error);process.exitCode=1;});
