// Actual bundled ARM GDB host constants. No inferior, remote, probe or board.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync,spawnSync}=require('node:child_process');
const {root,hash,outputDirectory,parseOptions,Cases}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--gdb','--elf'],[]);
const gdb=path.resolve(options.gdb||path.join(root,'tools/bin/gdb/bin/arm-none-eabi-gdb.exe'));
const out=outputDirectory('float-literals-arm-gdb'),commands=path.join(out,'constants.gdb');
const suite=new Cases(out,{layer:'actual ARM GDB host constants; no inferior',board_tests_executed:false,gdb,gdb_sha256:hash(gdb)});
const elf=options.elf?path.resolve(options.elf):null;
assert(elf,'--elf requires a debug ELF/object for the actual target ABI; no inferior will be started');
if(elf){suite.metadata.elf=elf;suite.metadata.elf_sha256=hash(elf);}
const target=elf?'file '+JSON.stringify(elf.replaceAll('\\','/'))+'\n':'';
fs.writeFileSync(commands,target+`set pagination off
set confirm off
set may-call-functions off
python
import gdb
assert gdb.parameter('may-call-functions') is False
assert all(i.pid == 0 for i in gdb.inferiors())
print('PASS-POLICY function_calls=false inferior_pid=0')
samples = {32: ('7f800000','ff800000','7fc00012','ffc54321','7f800001','ff800123'), 64: ('7ff0000000000000','fff0000000000000','7ff8000000000012','fff8fedcba987654','7ff0000000000001','fff0000000012345')}
name = '__debugtui_literal_offline'
for endian in ('little', 'big'):
    gdb.execute('set endian ' + endian)
    for bits, patterns in samples.items():
        type = gdb.lookup_type('float' if bits == 32 else 'double')
        assert type.sizeof == bits // 8
        for expected in patterns:
            raw = bytes.fromhex(expected)
            gdb.set_convenience_variable(name, gdb.Value(raw[::-1] if endian == 'little' else raw, type))
            expression = '(__typeof__($' + name + '))($' + name + ')'
            actual = gdb.execute('output/x ' + expression, to_string=True).strip()
            assert int(actual, 16) == int(expected, 16), (endian, expected, actual)
            gdb.set_convenience_variable(name, None)
            assert gdb.convenience_variable(name) is None
            print('PASS-LITERAL %s %s %s' % (endian, bits, expected))
end
show may-call-functions
quit
`);
(async()=>{
  let python=false;
  await suite.test('WRITE-T-VAR-ARM-GDB-CAPABILITY','Inspect actual GDB Python capability without a target',async()=>{
    const version=execFileSync(gdb,['--version'],{encoding:'utf8',windowsHide:true,timeout:10000});
    const capability=path.join(out,'capability.gdb');fs.writeFileSync(capability,target+'python import gdb\nquit\n');
    const probe=spawnSync(gdb,['-q','-nx','-batch','-x',capability],{encoding:'utf8',windowsHide:true,timeout:10000});
    if(probe.error)throw probe.error;
    python=probe.status===0;
    fs.writeFileSync(path.join(out,'capability.log'),version+'\n'+probe.stdout+probe.stderr);
    if(!python)assert(/(?:Python.*not supported|Scripting in.*Python.*not supported)/i.test(probe.stdout+probe.stderr),probe.stdout+probe.stderr);
    suite.metadata.python_buffer_api=python;
    return {version:version.split(/\r?\n/)[0],python_buffer_api:python,target_connected:false};
  });
  const arithmetic=path.join(out,'infinity.gdb');
  const pairs=[['((float)(0x1p127 + 0x1p127))','0x7f800000'],['(-((float)(0x1p127 + 0x1p127)))','0xff800000'],['((double)(0x1p1023 + 0x1p1023))','0x7ff0000000000000'],['(-((double)(0x1p1023 + 0x1p1023)))','0xfff0000000000000']];
  fs.writeFileSync(arithmetic,target+'set pagination off\nset may-call-functions off\n'+['little','big'].flatMap(endian=>['set endian '+endian,...pairs.flatMap(([value,expected])=>['echo RESULT '+endian+' '+expected+' = ','output/x '+value,'echo \\n'])]).join('\n')+'\nquit\n');
  await suite.test('WRITE-T-VAR-ARM-GDB-INFINITY','Actual GDB signed float/double Infinity literals in both byte orders',async()=>{
    const log=execFileSync(gdb,['-q','-nx','-batch','-x',arithmetic],{encoding:'utf8',windowsHide:true,timeout:30000});
    fs.writeFileSync(path.join(out,'infinity.log'),log);
    const results=[...log.matchAll(/^RESULT (little|big) (0x[0-9a-f]+) =\s*(0x[0-9a-f]+)/gm)];assert.equal(results.length,8);
    for(const result of results)assert.equal(BigInt(result[2]),BigInt(result[3]));return {constants_verified:8,target_connected:false};
  });
  if(!python)suite.skip('WRITE-T-VAR-ARM-GDB-LITERALS','Exact signed quiet/signaling NaN payloads in both byte orders','This GDB was built without Python; exact-buffer writes are rejected before assignment');
  else await suite.test('WRITE-T-VAR-ARM-GDB-LITERALS','Actual GDB preserves signed Infinity and quiet/signaling NaN payloads in both byte orders',async()=>{
    const log=execFileSync(gdb,['-q','-nx','-batch','-x',commands],{encoding:'utf8',windowsHide:true,timeout:30000});
    fs.writeFileSync(path.join(out,'gdb.log'),log);
    assert.equal((log.match(/^PASS-LITERAL /gm)||[]).length,24);
    assert(log.includes('PASS-POLICY function_calls=false inferior_pid=0'));
    return {constants_verified:24,target_connected:false};
  });suite.finish();
})();
