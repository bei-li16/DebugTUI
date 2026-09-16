// Development-only STM32 fixture test; no JavaScript runtime is needed by DebugTUI.
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const root = path.dirname(__dirname);
const elf = process.argv[2];
const binary = process.argv[3] || path.join(root, 'target/release/debugtui.exe');
const cycles = Number(process.argv[4] || 40);
const speed = process.argv[5];
if (speed && !/^[1-9][0-9]{0,4}$/.test(speed)) throw Error('SWD speed must be an integer in kHz');
if (!elf || !fs.existsSync(elf) || !Number.isInteger(cycles) || cycles < 1 || cycles > 1000) {
    throw Error('Usage: node scripts/test-step-hardware.cjs ELF [BINARY] [CYCLES=40] [SWD_KHZ]');
}
const output = path.join(root, 'artifacts', 'step-hardware-' + new Date().toISOString().replace(/[:.]/g, '-'));
fs.mkdirSync(output, { recursive: true });
const tomlPath = p => JSON.stringify(path.resolve(p).replaceAll('\\', '/'));
const config = path.join(output, 'project.toml');
fs.writeFileSync(config, `version=2\n[tools]\nprofile=${tomlPath(path.join(root, 'tools/debug-env.toml'))}\n[program]\nelf=${tomlPath(elf)}\n[session]\nlog_dir=${tomlPath(output)}\n`);
if (speed) fs.appendFileSync(config, `\n[service]\nargs=["-device","STM32F429IG","-if","SWD","-speed","${speed}","-port","3333","-select","USB","-localhostonly","-nogui","-halt","-singlerun"]\n`);
const events = fs.createWriteStream(path.join(output, 'events.jsonl'));
const timeline = [];
const child = spawn(binary, ['--project', config, '--headless', '--stdio'], { windowsHide: true });
let id = 0;
let snapshot;
let continueCycles = 0;
let stepCycles = 0;
let matchedSections = 0;
let sectionMismatch = false;
const pending = new Map();
const exit = new Promise(resolve => child.once('exit', resolve));
readline.createInterface({ input: child.stdout }).on('line', line => {
    events.write(line + '\n');
    const event = JSON.parse(line);
    if (event.event === 'snapshot') snapshot = event.snapshot;
    if (event.event === 'log' && event.channel === 'gdb') {
        if (/Section .*matched\./.test(event.text)) matchedSections++;
        if (/MIS-MATCH|does not match/i.test(event.text)) sectionMismatch = true;
    }
    if (event.event === 'response') pending.get(event.id)?.(event);
});
child.stderr.on('data', data => process.stderr.write(data));
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function command(method, params = {}) {
    const n = ++id;
    const start = Date.now();
    const response = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => { pending.delete(n); reject(Error(`No response: ${method}`)); }, 20000);
        pending.set(n, event => { clearTimeout(timer); pending.delete(n); resolve(event); });
        child.stdin.write(JSON.stringify({ id: n, method, params }) + '\n');
    });
    timeline.push({ id: n, method, ms: Date.now() - start, ...response });
    if (!response.ok) throw Error(`${n} ${method}: ${response.error}`);
    return response.result;
}
async function stoppedAt(method, func, line) {
    await command(method);
    // Exercise the same asynchronous stop notifications used by the TUI.
    // Do not send wait_stopped (its reconciliation could hide a missing event).
    const deadline = Date.now() + 6000;
    while (snapshot?.state !== 'STOPPED' && Date.now() < deadline) await delay(10);
    const state = await command('status');
    timeline.push({ after: method, reason: state.stop_reason, frame: state.frame });
    if (state.state !== 'STOPPED' || state.frame.function !== func || (line && state.frame.line !== line)) {
        throw Error(`${method}: expected ${func}:${line || '*'}, got ${state.state} ${JSON.stringify(state.frame)}`);
    }
    const xpsr = await command('evaluate', { expression: '$xpsr' });
    if (!(Number(xpsr.value) & 0x01000000)) throw Error(`Invalid Cortex-M xPSR: ${xpsr.value}`);
    return state;
}
(async () => {
    try {
        await command('connect');
        console.log('Initial frame:', JSON.stringify((await command('status')).frame));
        await command('console', { command: 'compare-sections -r' });
        if (sectionMismatch || matchedSections !== 6) throw Error(`ELF verification failed: ${matchedSections} matched sections`);
        await command('break', { location: 'user_task.c:40' });
        await command('break', { location: 'user_task.c:42' });
        await command('delete_break', { number: '2' });
        await stoppedAt('run', 'Task100ms', 40);
        for (let n = 0; n < cycles; n++) {
            await delay([0, 100, 500, 1500][n % 4]);
            await stoppedAt('continue', 'Task100ms', 40);
            continueCycles++;
            if ((n + 1) % 10 === 0) console.log(`PASS ${n + 1}/${cycles} repeated breakpoint hits`);
        }
        for (let n = 0; n < cycles; n++) {
            await stoppedAt('step', 'Led_Task');
            // The fixture ELF has VTASKDELAYUNTIL disabled: osDelay is on line 44.
            await stoppedAt('finish', 'Task100ms', 44);
            await stoppedAt('continue', 'Task100ms', 40);
            await stoppedAt('next', 'Task100ms', 44);
            await stoppedAt('next', 'Task100ms'); // Step over the RTOS delay/context switch.
            await stoppedAt('continue', 'Task100ms', 40);
            stepCycles++;
            if ((n + 1) % 10 === 0) console.log(`PASS ${n + 1}/${cycles} Step/Finish/Next sequences`);
        }
        console.log(`PASS ${cycles} continue cycles and ${cycles} Step/Next cycles, no reconnect or flash.`);
        fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify({ passed: true, continueCycles, stepCycles, speed: speed || 'profile', reconnects: 0 }, null, 2));
    } catch (error) {
        console.error(error.message);
        fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify({ passed: false, continueCycles, stepCycles, speed: speed || 'profile', error: error.message, frame: snapshot?.frame }, null, 2));
        process.exitCode = 1;
        for (const cmd of ['maintenance flush register-cache', 'info registers pc sp lr xpsr', 'x/4wx 0xE000ED28']) {
            try { await command('console', { command: cmd }); } catch (e) { console.error(e.message); }
        }
    } finally {
        try { await command('quit'); } catch (error) { console.error(error.message); child.kill(); process.exitCode = 1; }
        child.stdin.end();
        await exit;
        events.end();
        fs.writeFileSync(path.join(output, 'timeline.json'), JSON.stringify(timeline, null, 2));
        console.log(`Artifacts: ${output}`);
    }
})().catch(error => { console.error(error); child.kill(); process.exitCode = 1; });
