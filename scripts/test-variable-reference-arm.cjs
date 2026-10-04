// Bundled ARM GDB type/literal proof for the dedicated R52 C++ object.
// No inferior or remote target; this cannot prove a physical assignment.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const {root,hash,outputDirectory,parseOptions,Cases}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--object','--gdb'],[]);
assert(options.object,'--object requires the compiled variable-reference-board.cpp R52 EABI object');
const object=path.resolve(options.object),gdb=path.resolve(options.gdb||path.join(root,'tools/bin/gdb/bin/arm-none-eabi-gdb.exe'));
const out=outputDirectory('variable-reference-arm-types');
const suite=new Cases(out,{layer:'actual ARM GDB offline R52 C++ DWARF type/literal; no inferior',board_tests_executed:false,object,object_sha256:hash(object),gdb,gdb_sha256:hash(gdb)});
const commands=path.join(out,'types.gdb');
fs.writeFileSync(commands,`set pagination off
set may-call-functions off
file ${JSON.stringify(object.replaceAll('\\','/'))}
show architecture
ptype /r reference_value
print sizeof(reference_value)
whatis __typeof__(*(&(reference_value)))
print/x (__typeof__(*(&(reference_value))))(0x89abcdef)
print reference_fixture_has_int128
interpreter-exec mi "-gdb-show may-call-functions"
quit
`);
(async()=>{
  await suite.test('WRITE-T-VAR-R52-REFERENCE-TYPES','R52 DWARF reference resolves to 32-bit unsigned target type; generated constant requires no Python',async()=>{
    const log=execFileSync(gdb,['-q','-nx','-batch','-x',commands],{encoding:'utf8',windowsHide:true,timeout:30000});fs.writeFileSync(path.join(out,'gdb.log'),log);
    assert(log.includes('currently "armv8-r"'));assert(log.includes('type = unsigned int &'));assert(log.includes('$1 = 4'));assert(log.includes('$2 = 0x89abcdef'));assert(log.includes('^done,value="off"'));
    const wide=/\$3 = ([01])/.exec(log);assert(wide);return {reference_bits:32,compiler_has_int128:wide[1]==='1',target_connected:false,assignment_sent:false};
  });suite.finish();
})();
