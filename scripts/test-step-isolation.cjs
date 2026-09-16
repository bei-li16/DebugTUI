// Development-only test of the STM32 FreeRTOS fixture through GDB/MI directly.
// Each mode/session uses fresh GDB + J-Link processes; no TUI or firmware download.
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const root = path.dirname(__dirname);
const elf = process.argv[2];
const cycles = Number(process.argv[3] || 100);
const sessions = Number(process.argv[4] || 3);
const modes = (process.argv[5] || 'next,step,stepi,monitor').split(',');
const holdMs = Number(process.argv[6] || 0);
const execution = { next: '-exec-next', step: '-exec-step', stepi: '-exec-step-instruction', monitor: '-interpreter-exec console "monitor step"' };
const expected = { next: 0x080007e6, step: 0x080005fc, stepi: 0x080005f8, monitor: 0x080005f8 };
if (!elf || !fs.existsSync(elf) || !Number.isInteger(cycles) || cycles < 1 || cycles > 1000 ||
    !Number.isInteger(sessions) || sessions < 1 || sessions > 10 || modes.some(m => !execution[m]) ||
    !Number.isInteger(holdMs) || holdMs < 0 || holdMs > 120000) {
    throw Error('Usage: node scripts/test-step-isolation.cjs ELF [CYCLES=100] [SESSIONS=3] [next,step,stepi,monitor] [INITIAL_HALT_MS=0]');
}
const output = path.join(root, 'artifacts', 'step-isolation-' + new Date().toISOString().replace(/[:.]/g, '-'));
fs.mkdirSync(output, { recursive: true });
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const quote = value => JSON.stringify(value.replaceAll('\\', '/'));
const results = [];

async function test(mode, session) {
    const folder = path.join(output, `${session}-${mode}`);
    fs.mkdirSync(folder);
    const trace = fs.createWriteStream(path.join(folder, 'trace.log'));
    const timeline = [];
    const result = { mode, session, requested: cycles, holdMs, completed: 0, stage: 'startup', passed: false };
    const started = Date.now();
    const log = (channel, text) => trace.write(`${Date.now() - started}ms [${channel}] ${text}\n`);
    let server, gdb, serverExit, gdbExit;
    let seq = 0, stopSeq = 0, stop, lateError, serverReady = false;
    const pending = new Map();
    const stream = [];
    async function request(cmd, timeout = 6000) {
        const id = ++seq;
        const begin = Date.now();
        const streamAt = stream.length;
        log('mi>', `${id}${cmd}`);
        const record = await new Promise((resolve, reject) => {
            const timer = setTimeout(() => { pending.delete(id); reject(Error(`MI timeout: ${cmd}`)); }, timeout);
            pending.set(id, line => { clearTimeout(timer); pending.delete(id); resolve(line); });
            gdb.stdin.write(`${id}${cmd}\n`);
        });
        const response = { id, cmd, ms: Date.now() - begin, record, text: stream.slice(streamAt).join('') };
        timeline.push(response);
        if (/^\d+\^error/.test(record)) throw Error(record);
        return response;
    }
    const consoleCommand = cmd => request('-interpreter-exec console ' + JSON.stringify(cmd));
    async function execute(cmd) {
        const before = stopSeq;
        const response = await request(cmd);
        const deadline = Date.now() + 6000;
        while (stopSeq === before && lateError?.id !== response.id && Date.now() < deadline) await delay(5);
        if (lateError?.id === response.id) throw Error(lateError.line);
        if (stopSeq === before) throw Error(`Stop notification timeout: ${cmd}`);
        return stop;
    }
    async function registers() {
        const response = await request('-data-list-register-values x 13 14 15 25');
        const values = Object.fromEntries([...response.record.matchAll(/\{number="(\d+)",value="([^"]+)"\}/g)].map(m => [m[1], Number(m[2])]));
        return { sp: values[13], lr: values[14], pc: values[15], xpsr: values[25] };
    }
    function check(regs, pc) {
        result.lastRegisters = regs;
        const thumb = (regs.xpsr & 0x01000000) !== 0;
        const stack = regs.sp >= 0x20000000 && regs.sp <= 0x20030000 || regs.sp >= 0x10000000 && regs.sp <= 0x10010000;
        if (!thumb || !stack || !Number.isFinite(regs.pc)) throw Error(`Invalid register context: ${JSON.stringify(regs)}`);
        if (regs.pc !== pc) throw Error(`Unexpected PC: 0x${regs.pc.toString(16)}; expected 0x${pc.toString(16)}`);
    }
    try {
        server = spawn(path.join(root, 'tools/bin/jlink/JLinkGDBServerCL.exe'),
            ['-device', 'STM32F429IG', '-if', 'SWD', '-speed', '4000', '-port', '3333', '-select', 'USB', '-localhostonly', '-nogui', '-halt', '-singlerun'],
            { windowsHide: true });
        serverExit = new Promise(resolve => server.once('exit', resolve));
        server.once('error', e => { log('server-error', e.message); });
        readline.createInterface({ input: server.stdout }).on('line', line => {
            log('server', line);
            if (line.includes('Waiting for GDB connection') || line.includes('Connected to target')) serverReady = true;
        });
        server.stderr.on('data', data => log('server-stderr', data.toString()));
        const deadline = Date.now() + 15000;
        while (!serverReady && server.exitCode === null && Date.now() < deadline) await delay(25);
        if (!serverReady) throw Error('J-Link server did not become ready');
        const gdbDir = path.join(root, 'tools/bin/gdb');
        const env = { ...process.env }; delete env.PYTHONHOME; delete env.PYTHONPATH;
        gdb = spawn(path.join(gdbDir, 'bin/arm-none-eabi-gdb.exe'),
            ['-nx', '-q', '--interpreter=mi2', '--data-directory=' + path.join(gdbDir, 'arm-none-eabi/share/gdb')],
            { windowsHide: true, env });
        gdbExit = new Promise(resolve => gdb.once('exit', resolve));
        gdb.once('error', e => log('gdb-error', e.message));
        gdb.stderr.on('data', data => log('gdb-stderr', data.toString()));
        readline.createInterface({ input: gdb.stdout }).on('line', line => {
            log('mi<', line);
            const response = /^(\d+)\^/.exec(line);
            if (response) pending.get(Number(response[1]))?.(line);
            if (/^\d+\^error/.test(line)) lateError = { id: Number(response[1]), line };
            if (line.startsWith('*stopped')) { stop = line; stopSeq++; }
            if (/^[~@&]"/.test(line)) {
                try { stream.push(JSON.parse(line.slice(1))); } catch { stream.push(line.slice(1)); }
            }
        });
        for (const cmd of ['-gdb-set pagination off', '-gdb-set confirm off', '-gdb-set mi-async on',
            '-file-exec-and-symbols ' + quote(path.resolve(elf)), '-target-select extended-remote 127.0.0.1:3333']) await request(cmd);
        await consoleCommand('monitor reset');
        await consoleCommand('monitor halt');
        await consoleCommand('maintenance flush register-cache');
        const verify = await consoleCommand('compare-sections -r');
        if ((verify.text.match(/Section .*matched\./g) || []).length !== 6 || /MIS-MATCH/.test(verify.text)) throw Error('Read-only flash sections do not match ELF');
        await request('-break-insert "user_task.c:40"');
        for (let n = 1; n <= cycles; n++) {
            result.iteration = n;
            result.stage = 'continue-to-breakpoint';
            await execute('-exec-continue');
            check(await registers(), 0x080007e2);
            if (n === 1 && holdMs) {
                result.stage = 'holding-at-breakpoint';
                console.log(`Session ${session} ${mode}: holding at breakpoint for ${holdMs} ms without execution commands`);
                log('test', `Target halted at 0x080007e2; no commands for ${holdMs} ms`);
                await delay(holdMs);
                result.stage = 'read-after-hold';
                result.afterHoldMonitor = (await consoleCommand('monitor regs')).text;
                await consoleCommand('maintenance flush register-cache');
                check(await registers(), 0x080007e2);
            }
            result.stage = mode;
            if (mode === 'monitor') {
                await request(execution[mode]);
                const hardware = await consoleCommand('monitor regs');
                result.lastMonitor = hardware.text;
                // Monitor steps do not produce a GDB execution event. Explicitly
                // invalidate GDB's register cache before reading the new context.
                await consoleCommand('maintenance flush register-cache');
            } else {
                await execute(execution[mode]);
            }
            const regs = await registers();
            timeline.push({ iteration: n, mode, stop: mode === 'monitor' ? null : stop, regs });
            check(regs, expected[mode]);
            result.completed++;
            if (n % 25 === 0) console.log(`Session ${session} ${mode}: PASS ${n}/${cycles}`);
        }
        result.passed = true;
    } catch (error) {
        result.error = error.message;
        console.log(`Session ${session} ${mode}: FAIL ${result.stage}, iteration ${result.iteration || 0}: ${error.message}`);
        // Preserve evidence before any resume/reset/reconnect. In a responsive
        // stopped session, ask both the probe and GDB for the same context.
        if (gdb && gdb.exitCode === null && !/timeout/i.test(error.message)) {
            result.diagnostics = [];
            for (const cmd of ['monitor regs', 'maintenance flush register-cache', 'info registers pc sp lr xpsr',
                'monitor memU32 0xE000ED00', 'monitor memU32 0xE000EDF0', 'monitor memU32 0xE000ED28', 'monitor memU32 0xE000ED2C']) {
                try { result.diagnostics.push(await consoleCommand(cmd)); }
                catch (e) { result.diagnostics.push({ cmd, error: e.message }); if (/timeout/i.test(e.message)) break; }
            }
        }
    } finally {
        if (gdb && gdb.exitCode === null) {
            try {
                if (!/timeout/i.test(result.error || '')) {
                    await consoleCommand('delete breakpoints');
                    await consoleCommand('monitor go');
                    await request('-target-detach');
                }
                await request('-gdb-exit', 2000);
            } catch (e) { log('cleanup', e.message); }
            if (gdb.exitCode === null) gdb.kill();
            await gdbExit;
        }
        if (server && server.exitCode === null) {
            // SIGTERM is restricted to the child created by this session.
            await Promise.race([serverExit, delay(1500)]);
            if (server.exitCode === null) server.kill();
            await serverExit;
        }
        result.elapsedMs = Date.now() - started;
        result.folder = folder;
        fs.writeFileSync(path.join(folder, 'timeline.json'), JSON.stringify(timeline, null, 2));
        fs.writeFileSync(path.join(folder, 'result.json'), JSON.stringify(result, null, 2));
        await new Promise(resolve => trace.end(resolve));
    }
    return result;
}
(async () => {
    console.log(`Artifacts: ${output}`);
    for (let session = 1; session <= sessions; session++) {
        // Rotate order across sessions to avoid always testing one mode last.
        for (let n = 0; n < modes.length; n++) {
            const mode = modes[(n + session - 1) % modes.length];
            results.push(await test(mode, session));
            fs.writeFileSync(path.join(output, 'results.json'), JSON.stringify(results, null, 2));
        }
    }
    console.log(JSON.stringify(results.map(({ mode, session, completed, stage, passed, error }) => ({ mode, session, completed, stage, passed, error })), null, 2));
    if (results.some(r => !r.passed)) process.exitCode = 1;
})().catch(error => { console.error(error); process.exitCode = 1; });
