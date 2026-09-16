// Development-only stress test. No Node dependency is shipped in the TUI runtime.
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const root = path.dirname(__dirname);
const elf = process.argv[2];
const binary = process.argv[3] || path.join(root, 'target/release/debugtui.exe');
const cycles = Number(process.argv[4] || 100);
if (!elf || !fs.existsSync(elf) || !Number.isInteger(cycles) || cycles < 1 || cycles > 1000) {
    throw Error('Usage: node scripts/test-pause-hardware.cjs ELF [BINARY] [CYCLES=100]');
}
const output = path.join(root, 'artifacts', 'pause-hardware-' + new Date().toISOString().replace(/[:.]/g, '-'));
fs.mkdirSync(output, { recursive: true });
const tomlPath = p => JSON.stringify(path.resolve(p).replaceAll('\\', '/'));
const config = path.join(output, 'project.toml');
fs.writeFileSync(config, `version=2\n[tools]\nprofile=${tomlPath(path.join(root, 'tools/debug-env.toml'))}\n[program]\nelf=${tomlPath(elf)}\n[session]\nlog_dir=${tomlPath(output)}\n`);
const events = fs.createWriteStream(path.join(output, 'events.jsonl'));
const child = spawn(binary, ['--project', config, '--headless', '--stdio'], { windowsHide: true });
let id = 0;
let maxPauseMs = 0;
const pending = new Map();
const exit = new Promise(resolve => child.once('exit', resolve));
readline.createInterface({ input: child.stdout }).on('line', line => {
    events.write(line + '\n');
    const event = JSON.parse(line);
    if (event.event === 'response') pending.get(event.id)?.(event);
});
child.stderr.on('data', data => process.stderr.write(data));
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function command(method, params = {}) {
    const n = ++id;
    const response = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => { pending.delete(n); reject(Error(`No response: ${method}`)); }, 20000);
        pending.set(n, event => { clearTimeout(timer); pending.delete(n); resolve(event); });
        child.stdin.write(JSON.stringify({ id: n, method, params }) + '\n');
    });
    if (!response.ok) throw Error(`${n} ${method}: ${response.error}`);
    return response.result;
}
(async () => {
    try {
        await command('connect');
        await command('break', { location: 'user_task.c:40' });
        await command('run');
        const stopped = await command('wait_stopped');
        if (stopped.reason !== 'breakpoint-hit') throw Error('Did not hit the requested breakpoint');
        for (let n = 0; n < cycles; n++) {
            if (n % 4 === 0) { await command('next'); await command('wait_stopped'); }
            await command('continue');
            await delay([0, 5, 40, 150, 400][n % 5]);
            const start = Date.now();
            await command('pause');
            maxPauseMs = Math.max(maxPauseMs, Date.now() - start);
            const status = await command('status');
            if (status.state !== 'STOPPED' || !status.frame.address) throw Error('Pause did not restore a readable frame');
            if ((n + 1) % 25 === 0) console.log(`PASS ${n + 1}/${cycles} breakpoint/step/continue/pause cycles`);
        }
        fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify({ cycles, maxPauseMs, reconnects: 0, passed: true }, null, 2));
        console.log(`PASS ${cycles} cycles without reconnecting; maximum pause response ${maxPauseMs} ms.`);
    } catch (error) {
        console.error(error.message);
        process.exitCode = 1;
    } finally {
        try { await command('quit'); } catch (error) { console.error(error.message); child.kill(); process.exitCode = 1; }
        child.stdin.end();
        await exit;
        events.end();
        console.log(`Artifacts: ${output}`);
    }
})().catch(error => { console.error(error); child.kill(); process.exitCode = 1; });
