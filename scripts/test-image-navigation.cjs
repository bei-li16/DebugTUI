// Real bundled ARM GDB, real DWARF and conflicting CI paths. No board access.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),{execFileSync}=require('node:child_process');
const {Session,Cases,outputDirectory,quote,hash}=require('./test-support/session.cjs');
const binary=path.resolve(process.argv[2]||'target/debug/debugtui.exe'),out=outputDirectory('image-navigation');
const suite=new Cases(out,{board_tests_executed:false,binary,binary_sha256:hash(binary),layer:'Real ARM ELF/DWARF + bundled GDB; file-only queries, no physical inferior'});
const gdb=path.resolve('tools/bin/gdb/bin/arm-none-eabi-gdb.exe'),project=path.join(out,'debug.toml');
let session;
const normalized=s=>s.replaceAll('\\','/').toLowerCase();
const source=id=>path.join(out,'src-'+id,'main.c');
const dir=id=>path.join(out,'src-'+id);
const owner=file=>/^@([^:]+):(.*)$/.exec(file)?.slice(1);
(async()=>{try{
 if(!await suite.test('NAV-BUILD','Build three ARM ELFs whose same-name source records all refer to /ci/shared/main.c',async()=>{
  for(const [id,value]of [['bao',17],['vm0',33],['vm1',49]]){fs.mkdirSync(dir(id));fs.writeFileSync(source(id),`/* ${id} source identity */\nunsigned shared_global=${value};\n__attribute__((noinline)) unsigned shared_function(unsigned input){return input+${value};}\nvoid hvc_marker(void){__asm__ volatile("hvc #0");}\n`);}
  const build=`#!/usr/bin/env bash\nset -euo pipefail\nfor pair in bao:0x08001000 vm0:0x10020000 vm1:0x10028000; do\n id=\${pair%:*}; base=\${pair#*:}\n arm-none-eabi-gcc -mcpu=cortex-r52 -marm -mfloat-abi=soft -g -O0 -ffreestanding -nostdlib -Wl,-e,shared_function,-Ttext,\"$base\" -ffile-prefix-map=\"$PWD/src-$id=/ci/shared\" \"$PWD/src-$id/main.c\" -o \"$id.elf\"\ndone\narm-none-eabi-objcopy --strip-debug vm0.elf vm0-stripped.elf\n`;
  fs.writeFileSync(path.join(out,'build.sh'),build);execFileSync('wsl.exe',['-d','Ubuntu-24.04D','--cd',out,'--exec','bash','./build.sh'],{windowsHide:true,stdio:'pipe',timeout:120000});
  let config=`version=3\n[program]\nelf=${quote(path.join(out,'bao.elf'))}\nsource_root=${quote(dir('bao'))}\n[gdb]\nexecutable=${quote(gdb)}\ninit=['set architecture armv8-r']\n[target]\nmode='extended-remote'\n[session]\non_exit='disconnect'\n[[source_map]]\nfrom='/ci/shared'\nto=${quote(dir('bao'))}\n`;
  for(const id of ['bao','vm0','vm1'])config+=`[[images]]\nid='${id}'\nelf=${quote(path.join(out,id+'.elf'))}\nsource_root=${quote(dir(id))}\n${id==='bao'?'':`cores=['core.${id==='vm0'?0:1}']\n`}[[images.source_map]]\nfrom=${quote(id==='vm0'?'\\ci\\shared':'/ci/shared')}\nto=${quote(dir(id))}\n`;
  config+=`[[cores]]\nname='core.0'\nendpoint='fixture0'\n[[cores]]\nname='core.1'\nendpoint='fixture1'\n`;fs.writeFileSync(project,config);
  return Object.fromEntries(['bao','vm0','vm1'].map(id=>[id,hash(path.join(out,id+'.elf'))]));
 }))return;
 session=new Session(binary,project,out);
 if(!await suite.test('NAV-FILES','Each core lists the correct image-owned local file despite the conflicting global mapping',async()=>{
  const evidence=[];for(const n of [0,1]){await session.command('select_core',{name:'core.'+n});const result=await session.command('files');assert.equal(result.target_io,false);
   for(const id of ['bao','vm'+n]){const files=result.files.map(owner).filter(v=>v?.[0]===id&&v[1].endsWith('main.c'));assert.equal(files.length,1);assert.equal(normalized(files[0][1]),normalized(source(id)));}
   assert(!result.files.some(f=>f.startsWith('@vm'+(1-n)+':')));evidence.push(result);
  }assert.equal(session.logs('mi>').length,0);return evidence;
 }))return;
 if(!await suite.test('NAV-SYMBOLS','Same-name functions and globals retain image-owned source paths and can be explicitly filtered',async()=>{
  await session.command('select_core',{name:'core.0'});const all=await session.command('symbols',{query:'shared_function'});assert.equal(all.symbols.length,2);assert.equal(all.target_io,false);
  for(const s of all.symbols){const [id,file]=owner(s.file);assert.equal(normalized(file),normalized(source(id)));assert(s.description.startsWith('['+id+']'));}
  const only=await session.command('symbols',{query:'@vm0:shared_global'});assert.equal(only.symbols.length,1);assert(only.symbols[0].file.startsWith('@vm0:'));
  const mismatch=await session.command('symbols',{query:'@vm0:shared_global',image:'bao'},false);assert(mismatch.includes('disagree'));return {all,only,mismatch};
 }))return;
 if(!await suite.test('NAV-SOURCE-LOCATIONS','Mapped local file:line locations remain resolvable in their own objfile, including Windows drive colons',async()=>{
  const evidence=[];for(const id of ['bao','vm0']){const result=await session.command('image_locations',{image:id,location:source(id)+':3'});assert(result.locations.length>0);assert(result.locations.every(l=>id==='bao'?l.address>=0x08001000&&l.address<0x08001100:l.address>=0x10020000&&l.address<0x10020100));evidence.push(result);}assert.equal(session.logs('mi>').length,0);return evidence;
 }))return;
 if(!await suite.test('NAV-ASSEMBLY','Offline assembly comes from the selected ELF and labels its origin without a physical connection',async()=>{
  const evidence=[];for(const id of ['bao','vm0']){const result=await session.command('disassemble',{address:'@'+id+':shared_function',source:'elf'});assert.equal(result.target_io,false);assert.equal(result.image,id);assert.equal(result.source,'elf');assert(result.assembly[0].startsWith('ELF ['+id+']'));assert(result.assembly.length>2);evidence.push(result);}
  const hvc=await session.command('disassemble',{address:'@vm0:hvc_marker',source:'elf'});assert(hvc.assembly.some(line=>/\bhvc\b/.test(line)));assert.notDeepEqual(evidence[0].assembly,evidence[1].assembly);assert.equal(session.logs('mi>').length,0);return evidence;
 }))return;
 if(!await suite.test('NAV-RELOCATION','Relocated source/symbol/exec sections use the same runtime addresses in offline GDB disassembly',async()=>{
  const relocated=path.join(out,'relocated.toml');fs.writeFileSync(relocated,fs.readFileSync(project,'utf8').replace("id='vm0'","id='vm0'\noffset=65536"));const logs=path.join(out,'relocated');fs.mkdirSync(logs);const s=new Session(binary,relocated,logs);
  try{await s.command('select_core',{name:'core.0'});const location=await s.command('image_locations',{image:'vm0',location:'shared_function'});assert.equal(location.locations.length,1);const result=await s.command('disassemble',{address:'@vm0:shared_function',source:'elf'});assert.equal(result.start,location.locations[0].address);assert(result.start>=0x10030000);assert(result.assembly.length>2);assert.equal(s.logs('mi>').length,0);return {location,result};}finally{await s.close();}
 }))return;
 if(!await suite.test('NAV-MISSING-SOURCE','A missing per-image mapping never falls back to a same-name file in another source root',async()=>{
  const config=path.join(out,'missing-source.toml'),logs=path.join(out,'missing-source');fs.mkdirSync(logs);
  const text=fs.readFileSync(project,'utf8').replace('source_root='+quote(dir('vm0')),'source_root='+quote(dir('bao'))).replace('to='+quote(dir('vm0')),'to='+quote(path.join(out,'absent-vm0')));fs.writeFileSync(config,text);
  const s=new Session(binary,config,logs);try{await s.command('select_core',{name:'core.0'});const files=await s.command('files',{image:'vm0'});assert(files.files.some(f=>f==='@vm0:/ci/shared/main.c'));assert(!files.files.some(f=>normalized(f).includes(normalized(dir('bao')))));const symbols=await s.command('symbols',{query:'@vm0:shared_function'});assert.equal(symbols.symbols.length,1);assert.equal(symbols.symbols[0].file,'@vm0:/ci/shared/main.c');assert.equal(s.logs('mi>').length,0);return {files,symbols};}finally{await s.close();}
 }))return;
 if(!await suite.test('NAV-NO-DWARF','A stripped image reports missing DWARF while other images retain their own source records',async()=>{
  const config=path.join(out,'stripped.toml'),logs=path.join(out,'stripped');fs.mkdirSync(logs);fs.writeFileSync(config,fs.readFileSync(project,'utf8').replace(quote(path.join(out,'vm0.elf')),quote(path.join(out,'vm0-stripped.elf'))));const s=new Session(binary,config,logs);
  try{await s.command('select_core',{name:'core.0'});const files=await s.command('files');assert(files.files.some(f=>f.startsWith('@bao:')));assert(!files.files.some(f=>f.startsWith('@vm0:')));assert(files.warnings.some(w=>w.includes('vm0 has no DWARF')));const symbols=await s.command('symbols',{query:'@vm0:shared_function'});assert.equal(symbols.symbols.length,0);assert(symbols.warnings.some(w=>w.includes('vm0 has no DWARF')));assert.equal(s.logs('mi>').length,0);return {files,symbols};}finally{await s.close();}
 }))return;
}catch(error){await suite.test('NAV-SETUP','Execute real GDB source navigation regression',async()=>{throw error;});}
finally{if(session)await suite.test('NAV-CLEANUP','Close only owned metadata processes',()=>session.close());suite.finish();}})();
