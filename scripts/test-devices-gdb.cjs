// Chip-selected workspaces against real native GDBs; no board/probe required.
const {spawnSync} = require('node:child_process');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, quote:q, hash, outputDirectory, Cases, Session} = require('./test-support/session.cjs');
const binary=path.resolve(process.argv[2] || path.join(root,'target/debug/debugtui.exe'));
const out=outputDirectory('devices-gdb');
process.env.DEBUGTUI_CONFIG_DIR=path.join(out,'user');
const run=(file,args)=>{const r=spawnSync(file,args,{encoding:'utf8',windowsHide:true});assert.equal(r.status,0,r.stderr);return r.stdout;};
run(binary,['--init-profiles']);
const catalogue=path.join(process.env.DEBUGTUI_CONFIG_DIR,'profiles/devices.toml');
fs.appendFileSync(catalogue,'\n# Customer chip must survive install/upgrade.\n[devices.s32k144]\ncores=[0]\nbackend="generic"\n');
const preserved=fs.readFileSync(catalogue,'utf8');run(binary,['--init-profiles']);assert.equal(fs.readFileSync(catalogue,'utf8'),preserved);
fs.writeFileSync(path.join(out,'sample.c'),'volatile unsigned counter=17;\nint main(void){for(;;){counter++;}return 0;}\n');
run(process.env.DEBUGTUI_TEST_CC || 'C:/Program Files/mingw64/bin/gcc.exe',['-g','-O0',path.join(out,'sample.c'),'-o',path.join(out,'sample.exe')]);
fs.writeFileSync(path.join(out,'debug-env.toml'),`[gdb]\nexecutable=${q(process.env.DEBUGTUI_TEST_GDB || 'C:/Program Files/mingw64/bin/gdb.exe')}\n[target]\nmode="local"\nendpoint="local.0"\n[backends.tha6]\n[backends.stm32f4]\n[backends.generic]\n`+
  [0,1,2,3].map(id=>`[core_targets."${id}"]\nendpoint="local.${id}"\n`).join(''));
const tests=new Cases(out,{binary,sha256:hash(binary),type:'native GDB; simulated physical cores'});
(async()=>{
 for(const [chip,combinations] of [['tha6104',[[0]]],['tha6206',[[0],[1],[0,1]]],['tha6412',[[0],[1],[0,1],[0,1,2,3]]],['stm32f429',[[0]]],['s32k144',[[0]]]]) {
  for(const ids of combinations) await tests.test(`${chip}-${ids.join('-')}`,'selected core identities, group run/pause and saved per-chip preferences',async()=>{
   const directory=path.join(out,`${chip}-${ids.join('-')}`);fs.mkdirSync(directory);
   const project=path.join(directory,'debug.toml');
   fs.writeFileSync(project,`version=3\n[tools]\nprofile="../debug-env.toml"\n[debug]\nchip="${chip}"\ncores=${JSON.stringify(ids)}\n[program]\nelf="../sample.exe"\nsource_root=".."\n[session]\non_exit="disconnect"\n[multicore]\nscope="all"\nhalt_peers=false\n`);
   let s=new Session(binary,project,directory);
   try {
    const c=await s.command('connect');assert.deepEqual(c.core_names,ids.map(id=>`core.${id}`));
    for(const id of ids){await s.command('select_core',{name:`core.${id}`});await s.command('watch',{expression:'counter'});await s.command('break',{location:'main'});}
    await s.command('run');
    for(const id of ids){await s.command('select_core',{name:`core.${id}`});await s.command('wait_stopped');assert.equal((await s.command('status')).core.name,`core.${id}`);await s.command('delete_break',{number:''});}
    await s.command('continue');assert((await s.command('status')).cores.every(c=>c.state==='RUNNING'));
    await s.command('pause');assert((await s.command('status')).cores.every(c=>c.state==='STOPPED'));
   } finally {await s.close();}
   const saved=fs.readFileSync(project,'utf8');assert(!saved.includes('[[cores]]'));assert(saved.includes(`[core_preferences.${chip}."core.${ids[0]}"]`));
   s=new Session(binary,project,directory);
   try{
    await s.command('connect');await s.command('run');await s.command('pause');
    for(const id of ids){await s.command('select_core',{name:`core.${id}`});assert.deepEqual((await s.command('status')).watches.map(w=>w.name),['counter']);}
   }finally{await s.close();}
   return {cores:ids,project};
  });
 }
 tests.finish();
})().catch(error=>{console.error(error);process.exitCode=1;});
