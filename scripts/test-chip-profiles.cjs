#!/usr/bin/env node
'use strict';
// Offline checks against the actual CLI, copied tools and OpenOCD. Never connect/init.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {root, outputDirectory, parseOptions, Cases, hash, quote} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary']);
const binary = path.resolve(options.binary || path.join(root, 'target/release/debugtui.exe'));
const out = outputDirectory('chip-profiles');
let fixture = path.join(out, '移动工程 中文 空格');
let tools = path.join(fixture, '.vscode');
const config = path.join(out, 'user-config');
fs.mkdirSync(fixture, {recursive:true});
fs.mkdirSync(path.join(config, 'profiles'), {recursive:true});
fs.copyFileSync(path.join(root, 'profiles/devices.toml'), path.join(config, 'profiles/devices.toml'));
const catalogueHash = hash(path.join(config, 'profiles/devices.toml'));
const suite = new Cases(out, {binary, binary_sha256:hash(binary), board_tests_executed:false});
function run(name, executable, args, input) {
  const childEnv = {...process.env, DEBUGTUI_CONFIG_DIR:config};
  // Windows PowerShell 5 must initialize its own module paths when the test
  // was launched from PowerShell 7; otherwise even Get-FileHash may be absent.
  if (path.basename(executable).toLowerCase() === 'powershell.exe') {
    for (const key of Object.keys(childEnv)) if (key.toLowerCase() === 'psmodulepath') delete childEnv[key];
  }
  const result = spawnSync(executable, args, {input, cwd:out, encoding:'utf8', windowsHide:true, timeout:60000,
    maxBuffer:8*1024*1024, env:childEnv});
  fs.writeFileSync(path.join(out, name + '.stdout.txt'), result.stdout || '');
  fs.writeFileSync(path.join(out, name + '.stderr.txt'), result.stderr || '');
  assert.ifError(result.error);
  return result;
}
function status(name, profile, chip, cores) {
  const project = path.join(fixture, name + '.toml');
  fs.writeFileSync(project, `version=3\n[tools]\nprofile=${quote(path.relative(fixture,profile))}\n` +
    (chip ? `[debug]\nchip=${quote(chip)}\ncores=${JSON.stringify(cores)}\n` : ''));
  const before = [hash(project), hash(profile)];
  const result = run(name, binary, ['--project', project, '--headless', '--stdio'], '{"id":1,"method":"status"}\n');
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual([hash(project), hash(profile)], before, 'Loading must preserve project/profile files');
  const events = result.stdout.trim().split(/\r?\n/).map(JSON.parse);
  assert(!events.some(e => e.event === 'log' && e.channel === 'mi>'));
  const response = events.find(e => e.event === 'response' && e.id === 1);
  assert.equal(response?.ok, true, JSON.stringify(response));
  if (chip) {
    assert(response.result.cores.every(core => core.state === 'DISCONNECTED'));
    assert.deepEqual(response.result.cores.map(({name, endpoint}) => ({name, endpoint})),
      cores.map(id => ({name:`core.${id}`, endpoint:`127.0.0.1:${3333+id}`})));
  } else {
    // The legacy single-core protocol has a root state and no multicore envelope.
    assert.equal(response.result.state, 'DISCONNECTED');
    assert.equal(response.result.cores, undefined);
  }
  assert(events.some(e => e.event === 'exit'));
  assert.equal(hash(path.join(config, 'profiles/devices.toml')), catalogueHash);
  return {state:response.result.state, cores:response.result.cores};
}
(async () => {
  try {
    if (!await suite.test('CHIP-FILES-INSTALL', 'Tools installer copies chip families and probes into a path containing Chinese and spaces', async () => {
      const result = run('tools-install', 'powershell.exe', ['-NoProfile','-ExecutionPolicy','Bypass','-File',path.join(root,'tools/install.ps1'),fixture,'cmsis-dap']);
      assert.equal(result.status, 0, result.stderr);
      const expected = ['debug-env.toml','devices/stm32f429.toml','devices/tha6104.toml','devices/tha6206.toml',
        'devices/tha6412.toml','devices/families/cortex-r52.toml','openocd/probes/cmsis-dap.cfg','openocd/probes/jlink.cfg','openocd/probes/stlink.cfg',
        'openocd/stm32f429.cfg','openocd/r52-template.cfg','svd/STM32F429.svd'];
      for (const file of expected) assert.equal(hash(path.join(tools,file)),hash(path.join(root,'tools',file)));
      assert.deepEqual(fs.readdirSync(tools).filter(name => /^debug-env.*\.toml$/.test(name)), ['debug-env.toml']);
      assert.deepEqual(fs.readdirSync(path.join(tools,'openocd')).filter(name => name.startsWith('stm32')), ['stm32f429.cfg']);
      for (const obsolete of ['chip','chips','config','probes','debug.toml','debug-project.toml.example','install.ps1','install.bat','package.ps1','dependencies.lock.json']) assert(!fs.existsSync(path.join(tools,obsolete)));
      function checkBinaries(directory, relative = 'bin') {
        for (const entry of fs.readdirSync(directory,{withFileTypes:true})) {
          const name = path.join(relative,entry.name);
          if (entry.isDirectory()) checkBinaries(path.join(directory,entry.name),name);
          else assert.equal(hash(path.join(tools,name)),hash(path.join(root,'tools',name)),name);
        }
      }
      checkBinaries(path.join(root,'tools/bin'));
      assert.deepEqual(fs.readdirSync(path.join(root,'tools')).filter(name => /\.(ps1|bat)$/i.test(name)), ['install.ps1']);
      const project = fs.readFileSync(path.join(fixture,'debug.toml'),'utf8');
      assert(project.includes('version = 3') && project.includes('chip = ""') && !/^svd\s*=/m.test(project));
      assert.equal(project.replaceAll('\r\n','\n'),fs.readFileSync(path.join(root,'tools/debug.toml'),'utf8').replaceAll('\r\n','\n').replace('builtin:arm-openocd','./.vscode/debug-env.toml'));
      assert(project.includes('profile = "./.vscode/debug-env.toml"'));
      function checkPortable(directory) {
        for (const entry of fs.readdirSync(directory,{withFileTypes:true})) {
          const file = path.join(directory,entry.name);
          if (entry.isDirectory()) checkPortable(file);
          else if (entry.name.endsWith('.toml')) assert(!/["'][A-Za-z]:[\\/]|["']\\\\/.test(fs.readFileSync(file,'utf8')), `Absolute configuration path in ${file}`);
        }
      }
      checkPortable(fixture);
      return {files:expected};
    })) return;
    if (!await suite.test('CHIP-FILES-RELOCATE', 'Move the entire installed project and load its relative profile from an unrelated working directory', async () => {
      const destination = path.join(out,'重新定位 项目');
      for (const directory of [fixture,destination]) assert(path.resolve(directory).startsWith(path.resolve(out)+path.sep));
      fs.renameSync(fixture,destination);
      fixture = destination;
      tools = path.join(fixture,'.vscode');
      const project = path.join(fixture,'debug.toml');
      const selected = fs.readFileSync(project,'utf8').replace('chip = ""','chip = "stm32f429"').replace('cores = []','cores = [0]');
      fs.writeFileSync(project,selected);
      const before = hash(project);
      const result = run('relocated-project',binary,['--project',fixture,'--headless','--stdio'],'{"id":1,"method":"status"}\n');
      assert.equal(result.status,0,result.stderr);
      const response = result.stdout.trim().split(/\r?\n/).map(JSON.parse).find(e => e.event === 'response' && e.id === 1);
      assert.equal(response?.ok,true,JSON.stringify(response));
      assert.equal(response.result.cores[0].state,'DISCONNECTED');
      assert.equal(hash(project),before);
      return {directory:fixture,relative_profile_loaded:true};
    })) return;
    const profile = path.join(tools, 'debug-env.toml');
    const common = fs.readFileSync(profile, 'utf8');
    assert(!common.includes('[backends.') && !common.includes('stm32') && !common.includes('tha6'));
    for (const probe of ['cmsis-dap', 'jlink', 'stlink']) {
      fs.writeFileSync(profile, common.replace('./openocd/probes/cmsis-dap.cfg', `./openocd/probes/${probe}.cfg`));
      await suite.test(`CHIP-FILES-STM32-${probe}`, 'Same project shape and common profile with an independent probe', async () =>
        status(`stm32-${probe}`, profile, 'stm32f429', [0]));
    }
    fs.writeFileSync(profile, common);
    for (const [chip, cores] of [['tha6104',[0]],['tha6206',[1]],['tha6206',[0,1]],['tha6412',[1,3]],['tha6412',[0,1,2,3]]]) {
      const name = `${chip}-${cores.join('')}`;
      await suite.test(`CHIP-FILES-${name}`, 'Inherited R52 template keeps physical core IDs and ports', async () => status(name, profile, chip, cores));
    }
    const openocd = path.join(tools, 'bin/openocd/bin/openocd.exe');
    for (const probe of ['cmsis-dap','jlink','stlink']) {
      await suite.test(`CHIP-FILES-OCD-${probe}`, 'OpenOCD parses separate adapter and STM32 board scripts without init', async () => {
        const result = run(`openocd-${probe}`, openocd, ['-s',path.join(tools,'bin/openocd/scripts'),'-f',path.join(tools,`openocd/probes/${probe}.cfg`),'-f',path.join(tools,'openocd/stm32f429.cfg'),'-c','shutdown']);
        assert.equal(result.status, 0, result.stderr);
        return {exit_code:result.status};
      });
    }
    await suite.test('CHIP-FILES-R52-GUARD', 'Unconfigured R52 board template rejects startup with a specific message', async () => {
      const result = run('r52-template', openocd, ['-s',path.join(tools,'bin/openocd/scripts'),'-f',path.join(tools,'openocd/probes/cmsis-dap.cfg'),'-f',path.join(tools,'openocd/r52-template.cfg'),'-c','shutdown']);
      assert.notEqual(result.status, 0);
      assert((result.stdout + result.stderr).includes('R52 board template is not configured'));
      return {exit_code:result.status, rejected_as_expected:true};
    });
    await suite.test('CHIP-FILES-LEGACY', 'Old configuration syntax stays supported without shipping duplicate profiles', async () => {
      const legacy = path.join(fixture,'legacy-fixture.toml');
      fs.writeFileSync(legacy, '[gdb]\nexecutable="must-not-start-gdb.exe"\n[target]\nmode="remote"\nendpoint="localhost:3333"\n');
      return status('legacy', legacy);
    });
    await suite.test('CHIP-FILES-UPGRADE', 'Installer migrates generated SVD defaults and preserves project preferences', async () => {
      const project = path.join(fixture,'debug.toml');
      fs.mkdirSync(path.join(tools,'chip'),{recursive:true});
      fs.copyFileSync(path.join(tools,'svd/STM32F429.svd'),path.join(tools,'chip/STM32F429.svd'));
      const original = '  version = 2 # customer format\nwatch=["customer.counter"]\n[tools]\nprofile="./.vscode/debug-env-cmsis-dap.toml"\n[debug]\nchip="stm32f429"\ncores=[0]\n[program]\nelf="build/customer.elf"\nsvd="./.vscode/chip/STM32F429.svd"\n';
      fs.writeFileSync(project,original);
      fs.writeFileSync(path.join(tools,'dependencies.lock.json'),'obsolete manifest; no longer parsed');
      const result = run('upgrade-install','powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File',path.join(root,'tools/install.ps1'),fixture,'jlink']);
      assert.equal(result.status,0,result.stderr);
      assert(!fs.existsSync(path.join(tools,'dependencies.lock.json')));
      const updated = fs.readFileSync(project,'utf8');
      for (const preserved of ['watch=["customer.counter"]','chip="stm32f429"','cores=[0]','elf="build/customer.elf"']) assert(updated.includes(preserved));
      assert(updated.includes('  version = 3 # customer format') && updated.includes('profile = "./.vscode/debug-env.toml"'));
      assert(!/^svd\s*=/m.test(updated));
      assert.equal(fs.readFileSync(project+'.bak','utf8'),original);
      assert(fs.readFileSync(profile,'utf8').includes('./openocd/probes/jlink.cfg'));
      const state = run('upgraded-project',binary,['--project',project,'--headless','--stdio'],'{"id":1,"method":"status"}\n');
      assert.equal(state.status,0,state.stderr);
      return {project_preferences_preserved:true,stock_svd_follows_chip:true};
    });
    await suite.test('CHIP-FILES-CUSTOM-SVD', 'Installer preserves a modified SVD and its explicit project override', async () => {
      const project = path.join(fixture,'debug.toml');
      const custom = path.join(tools,'svd/STM32F429.svd');
      fs.appendFileSync(custom,'\n<!-- customer extension marker -->\n');
      const before = hash(custom);
      fs.appendFileSync(project,'svd="./.vscode/svd/STM32F429.svd"\n');
      const result = run('custom-svd-install','powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File',path.join(root,'tools/install.ps1'),fixture,'cmsis-dap']);
      assert.equal(result.status,0,result.stderr);
      assert.equal(hash(custom),before);
      assert(fs.readFileSync(project,'utf8').includes('svd="./.vscode/svd/STM32F429.svd"'));
      return {custom_svd_preserved:true};
    });
    await suite.test('CHIP-FILES-ELF-RELATIVE', 'PowerShell 7 installer converts absolute ELF arguments inside and outside the project into relative paths', async () => {
      const project = path.join(fixture,'debug.toml');
      const backup = hash(project+'.bak');
      const paths = [path.join(fixture,'build','firmware #100%.elf'),path.join(out,'共享固件 #100%.elf')];
      for (const [index,elf] of paths.entries()) {
        const result = run(`relative-elf-${index}`,'pwsh.exe',['-NoProfile','-File',path.join(root,'tools/install.ps1'),fixture,'cmsis-dap',elf]);
        assert.equal(result.status,0,result.stderr);
        const relative = path.relative(fixture,elf).replaceAll('\\','/');
        const expected = relative.startsWith('../') ? relative : './'+relative;
        assert(fs.readFileSync(project,'utf8').includes(`elf = "${expected}"`));
        assert.equal(hash(project+'.bak'),backup);
      }
      return {relative_elf_arguments:paths.length,original_backup_preserved:true};
    });
    await suite.test('CHIP-FILES-CROSS-DRIVE', 'Reject an ELF that cannot be relative before changing project files', async () => {
      const project = path.join(fixture,'debug.toml');
      const profileBefore = hash(profile), projectBefore = hash(project);
      const otherDrive = path.parse(fixture).root.toUpperCase() === 'C:\\' ? 'G:\\' : 'C:\\';
      const result = run('cross-drive-elf','powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File',path.join(root,'tools/install.ps1'),fixture,'stlink',path.join(otherDrive,'firmware.elf')]);
      assert.notEqual(result.status,0);
      assert((result.stdout+result.stderr).includes('ELF must be on the same'));
      assert.equal(hash(project),projectBefore);
      assert.equal(hash(profile),profileBefore);
      return {rejected_before_changes:true};
    });
  } catch (error) {
    await suite.test('CHIP-FILES-SETUP', 'Prepare offline configuration checks', async () => {throw error;});
  } finally {suite.finish();}
})();
