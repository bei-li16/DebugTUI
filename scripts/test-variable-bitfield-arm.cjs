// Load actual R52 ELF objects, inspect target layouts/data. No inferior or board.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const {root,hash,outputDirectory,parseOptions,Cases}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--little-object','--big-object','--gdb']);
assert(options['little-object']&&options['big-object'],'Provide independently compiled little/big endian R52 objects');
const gdb=path.resolve(options.gdb||path.join(root,'tools/bin/gdb/bin/arm-none-eabi-gdb.exe')),out=outputDirectory('variable-bitfield-arm');
const suite=new Cases(out,{layer:'actual R52 ELF/DWARF in bundled ARM GDB; offline, no target writes',board_tests_executed:false,gdb,gdb_sha256:hash(gdb)});
(async()=>{
 for(const little of [true,false])await suite.test(little?'BF-T-ARM-LITTLE':'BF-T-ARM-BIG','Actual ARM field offsets, byte order and raw section data without Python',async()=>{
  const object=path.resolve(options[little?'little-object':'big-object']),script=path.join(out,little?'little.gdb':'big.gdb');
  fs.writeFileSync(script,'set may-call-functions off\nptype /rod bitfield_cells\nptype /rod bitfield_packed\nshow endian\nprint sizeof(bitfield_cells)\nprint/x bitfield_cells.middle\necho BITFIELD_BYTES_BEGIN\\n\nx/12bx &bitfield_cells\necho BITFIELD_BYTES_END\\n\nshow configuration\n');
  const output=execFileSync(gdb,['-q','-batch',object,'-x',script],{encoding:'utf8',windowsHide:true});fs.writeFileSync(path.join(out,little?'little.log':'big.log'),output);
  assert(output.includes(`currently ${little?'little':'big'} endian`));assert(output.includes('--without-python'));
  assert(/4:\s*5\s*\|\s*4\s*\*\/\s*int middle\s*:\s*6/.test(output));assert(/total size \(bytes\):\s*12/.test(output));assert(/total size \(bytes\):\s*5/.test(output));
  const section=output.split('BITFIELD_BYTES_BEGIN')[1].split('BITFIELD_BYTES_END')[0],bytes=[];
  for(const line of section.split(/\r?\n/))if(/^0x[0-9a-f]+\s.*:/.test(line))bytes.push(...[...line.slice(line.indexOf(':')+1).matchAll(/0x([0-9a-f]{2})(?![0-9a-f])/g)].map(m=>parseInt(m[1],16)));
  assert.equal(bytes.length,12);assert.deepEqual(bytes.slice(4,8),little?[0xc3,0xaf,0xf2,0xaa]:[0x1f,0xd5,0x6a,0xbc]);
  let raw=0;for(let i=0;i<6;i++){const pos=37+i,bit=(bytes[Math.floor(pos/8)]>>(little?pos%8:7-pos%8))&1;raw|=bit<<(little?i:5-i);}assert.equal(raw,62);assert(/0xfffffffe/.test(output));
  return {object,object_sha256:hash(object),little_endian:little,layout:{bit_offset:37,bits:6,declared_bytes:4,parent_bytes:12},bytes,raw_value:raw,python_required:false,target_assignment_executed:false};
 });suite.finish();
})();
