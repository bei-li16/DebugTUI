// Explicit Bao task regression. Fixture checks never access a debug probe.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {outputDirectory, Cases, hash} = require('./test-support/session.cjs');
const bao = path.resolve(process.argv[2] || '');
assert(process.argv[2], 'Usage: node scripts/test-bao-tasks.cjs BAO_ROOT');
const out = outputDirectory('bao-tasks');
const suite = new Cases(out, {bao, mode: 'fixtures-only; no board access'});
const fixture = path.join(out, 'fixture project 空格');
const scripts = path.join(fixture, 'scripts');
const images = path.join(fixture, 'bin/tha6xxx/tha6206-smoke');
fs.mkdirSync(scripts, {recursive: true});
fs.mkdirSync(images, {recursive: true});
for (const file of ['tha6206-build.ps1', 'tha6206-download.ps1']) {
  fs.copyFileSync(path.join(bao, 'scripts', file), path.join(scripts, file));
}
for (const file of ['bao.elf', 'bao.bin', 'bao.hex']) fs.writeFileSync(path.join(images, file), file);
const manifest = path.join(images, 'bao.sha256');
const programmer = path.join(out, 'THA6XXX_MC_AS440/.vscode/THA6206');
fs.mkdirSync(programmer, {recursive: true});
fs.writeFileSync(path.join(programmer, 'jtag.cs'), `using System;
public class ProgrammerFixture {
  public static int Main(string[] args) {
    Console.OutputEncoding = new System.Text.UTF8Encoding(false);
    Console.WriteLine("fixture programmer: " + string.Join(" ", args));
    Console.Error.WriteLine("fixture native stderr warning");
    return int.Parse(Environment.GetEnvironmentVariable("DEBUGTUI_TEST_PROGRAMMER_EXIT") ?? "0");
  }
}`);
fs.writeFileSync(path.join(programmer, 'compile.ps1'),
  "$ErrorActionPreference='Stop'\nAdd-Type -Path (Join-Path $PSScriptRoot 'jtag.cs') -OutputAssembly (Join-Path $PSScriptRoot 'jtag.exe') -OutputType ConsoleApplication\n");
const compiler = spawnSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(programmer, 'compile.ps1')],
  {encoding: 'utf8', windowsHide: true, timeout: 60000});
assert.ifError(compiler.error); assert.equal(compiler.status, 0, compiler.stdout + compiler.stderr);
const writeManifest = () => fs.writeFileSync(manifest,
  ['bao.elf', 'bao.bin', 'bao.hex'].map(file => `${hash(path.join(images, file))}  ${file}\n`).join(''));
function run(script, args = [], env = {}) {
  const result = spawnSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(scripts, script), ...args],
    {encoding: 'utf8', windowsHide: true, timeout: 60000, env: {...process.env, ...env}});
  assert.ifError(result.error);
  const output = result.stdout + result.stderr;
  fs.appendFileSync(path.join(out, 'tasks.log'), JSON.stringify({script, args, code: result.status, output}) + '\n');
  return {code: result.status, output};
}
(async () => {
  await suite.test('missing-manifest', 'Download refuses images without a successful build', () => {
    const result = run('tha6206-download.ps1', ['-VerifyOnly']);
    assert.equal(result.code, 1); assert.match(result.output, /Run Build successfully/);
  });
  await suite.test('matching-images', 'Validate ELF/BIN/HEX in a directory containing spaces and Unicode', () => {
    writeManifest(); const result = run('tha6206-download.ps1', ['-VerifyOnly']);
    assert.equal(result.code, 0, result.output); assert.match(result.output, /Verified matching/);
  });
  await suite.test('changed-elf', 'Download refuses an ELF that no longer matches the HEX build', () => {
    fs.appendFileSync(path.join(images, 'bao.elf'), 'changed');
    const result = run('tha6206-download.ps1', ['-VerifyOnly']);
    assert.equal(result.code, 1); assert.match(result.output, /Build output mismatch: bao.elf/);
  });
  await suite.test('failed-build', 'Failed WSL invocation preserves its error and invalidates old build approval', () => {
    writeManifest();
    const result = run('tha6206-build.ps1', ['-WslDistribution', 'debugtui-test-nonexistent-' + process.pid]);
    assert.equal(result.code, 1); assert.match(result.output, /WSL_E_DISTRO_NOT_FOUND/);
    assert(!result.output.includes('\uFFFD'), 'UTF-8 output was corrupted');
    assert(!fs.existsSync(manifest), 'Stale build manifest survived');
    assert.equal(run('tha6206-download.ps1', ['-VerifyOnly']).code, 1);
    return {originalWslErrorVisible: true};
  });
  await suite.test('programmer-success', 'Native stderr does not mask success and both stages record exit code zero', () => {
    writeManifest();
    const result = run('tha6206-download.ps1', [], {DEBUGTUI_TEST_PROGRAMMER_EXIT: '0'});
    assert.equal(result.code, 0, result.output);
    assert.equal((result.output.match(/Programmer exit: 0/g) || []).length, 2);
    assert.match(result.output, /fixture native stderr warning/);
    assert.match(result.output, /Download and system reset completed/);
  });
  await suite.test('programmer-failure', 'A native tool failure propagates and never triggers the reset stage', () => {
    const result = run('tha6206-download.ps1', [], {DEBUGTUI_TEST_PROGRAMMER_EXIT: '23'});
    assert.equal(result.code, 1); assert.match(result.output, /THA6206 programmer failed \(23\)/);
    assert(!result.output.includes('jtag.exe --reset'), 'Reset ran after failed flashing');
  });
  suite.finish();
})().catch(error => {console.error(error); process.exitCode = 1;});
