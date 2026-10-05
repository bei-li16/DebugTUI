// Actual C++ GDB reference and 128-bit typed assignments. No board.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const {root,hash,outputDirectory,Cases,Session}=require('./test-support/session.cjs');
const out=outputDirectory('variable-reference-native');
const binary=path.resolve(process.argv[2]||path.join(root,'target/debug/debugtui.exe'));
const cc=process.env.DEBUGTUI_TEST_CXX||(process.env.DEBUGTUI_TEST_CC||'C:/MinGW/bin/gcc.exe').replace(/gcc(?=\.exe$|$)/,'g++').replace(/clang(?=\.exe$|$)/,'clang++');
const gdb=process.env.DEBUGTUI_TEST_GDB||'C:/MinGW/bin/gdb.exe';
const suite=new Cases(out,{layer:'native C++ GCC/GDB local process; no board',board_tests_executed:false,binary,binary_sha256:hash(binary),gdb,cc});
const ids=['WRITE-T-VAR-REFERENCE','WRITE-T-VAR-REFERENCE-MEMBER','WRITE-T-VAR-REFERENCE-QUALIFIERS','WRITE-T-VAR-REFERENCE-FLOAT','WRITE-T-VAR-128','WRITE-T-VAR-REFERENCE-FRAME','WRITE-T-VAR-REFERENCE-CLEANUP'];
ids.push('WRITE-T-VAR-BITFIELD-DENIED');
const source=path.join(out,'sample.cpp'),exe=path.join(out,'sample.exe'),project=path.join(out,'project.toml');
fs.writeFileSync(source,`
#include <stdint.h>
struct Cells { uint32_t before; uint32_t value; uint32_t after; } cells={0x12345678,7,0x87654321};
uint32_t &reference=cells.value;
uint32_t &&rvalue_reference=static_cast<uint32_t&&>(cells.value);
const uint32_t immutable=11;
const uint32_t &const_reference=immutable;
volatile uint32_t volatile_value=13;
volatile uint32_t &volatile_reference=volatile_value;
const uint32_t *pointer_to_const=&immutable;
const uint32_t *&pointer_reference=pointer_to_const;
struct ReferenceBox {uint32_t before; uint32_t &member; uint32_t after;};
ReferenceBox box={0x11223344,cells.value,0x55667788};
ReferenceBox &box_reference=box;
struct Wide { uint64_t before; alignas(16) unsigned __int128 value; uint64_t after; } wide={0x1122334455667788ULL,1,0x8877665544332211ULL};
__int128 signed_wide=1;
float single=1.0f;
float &float_reference=single;
struct BitfieldObject {uint32_t before;unsigned low:5;signed middle:6;unsigned neighbour:7;unsigned reserved:14;uint32_t after;} bit_object={0x12345678,3,-2,0x55,0x2abc,0x87654321};
unsigned calls=0;
int side_effect(){++calls;return 42;}
void checkpoint(){calls+=0;}
void caller(){uint32_t local=17;uint32_t &local_reference=local;checkpoint();cells.value+=local*0;}
int main(){caller();return 0;}
`);
execFileSync(cc,['-g','-O0',source,'-o',exe],{windowsHide:true});
const quote=x=>JSON.stringify(x.replaceAll('\\','/'));
fs.writeFileSync(project,`version=2\nwatch=${JSON.stringify(['reference','rvalue_reference','const_reference','volatile_reference','pointer_reference','box','box_reference','wide','signed_wide','float_reference','bit_object'])}\n[[breakpoints]]\nlocation='checkpoint'\nkind='code'\n[gdb]\nexecutable=${quote(gdb)}\n[target]\nmode='local'\n[program]\nelf=${quote(exe)}\n[session]\non_exit='disconnect'\n[[writes.regions]]\nid='native-reference-ram'\nkind='ram'\nstart='0x1'\nend='0xffffffffffffffff'\nwidths=[8,16,32,64,128]\nscope='core'\n`);
const originalHash=hash(project);let session;
const context=async()=>(await session.command('registers_list')).context;
const preview=async(expression,text,kind='unsigned',path=[])=>session.command('write_preview',{target:{kind:'variable',pane:'watch',expression,path},selection:{kind:'register'},input:{kind,text,...(kind==='bytes'?{little_endian:true}:{})},context:await context()});
const evaluate=async expression=>(await session.command('evaluate',{expression})).value;
const number=async expression=>BigInt((await evaluate(expression)).split(' ')[0]);
const assignments=()=>session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length;
const apply=async draft=>{const count=assignments();const result=await session.command('write_apply',{draft:draft.draft});assert.equal(result.outcome,'verified',JSON.stringify(result));assert.equal(assignments(),count+1);assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');assert.equal(assignments(),count+1);return result;};
(async()=>{
  try{
    session=new Session(binary,project,out);await session.command('connect');await session.command('run');await session.command('wait_stopped');
    if(!await suite.test(ids[0],'Lvalue/rvalue references write referents, cancel does not write, binding remains unchanged',async()=>{
      const address=await evaluate('(unsigned long long)&reference'),count=assignments();
      const cancelled=await preview('reference','31');assert.equal(cancelled.metadata.reference,true);assert.equal(cancelled.metadata.address,'0x'+BigInt(address).toString(16));
      await session.command('write_cancel',{draft:cancelled.draft});assert.equal((await session.command('write_apply',{draft:cancelled.draft})).outcome,'not_sent');assert.equal(assignments(),count);assert.equal(await number('cells.value'),7n);
      const first=await apply(await preview('reference','4294967295'));assert.equal(await number('cells.value'),4294967295n);
      const second=await apply(await preview('rvalue_reference','19'));assert.equal(await number('cells.value'),19n);assert.equal(await evaluate('(unsigned long long)&reference'),address);return {address,first,second};
    }))return;
    if(!await suite.test(ids[1],'Reference member and aggregate-reference paths retain real referent addresses',async()=>{
      const evidence=[];
      for(const expression of ['box','box_reference']){
        await session.command('watch_expand',{expression,path:[],expanded:true});await session.command('watch_expand',{expression,path:[0],expanded:true});
        const draft=await preview(expression,'23','unsigned',[0,1]);assert.equal(draft.metadata.reference,true);assert.equal(BigInt(draft.metadata.address),await number('(unsigned long long)&cells.value'));evidence.push(await apply(draft));assert.equal(await number('cells.value'),23n);
      }
      assert.equal(await number('box.before'),0x11223344n);assert.equal(await number('box.after'),0x55667788n);return evidence;
    }))return;
    if(!await suite.test(ids[2],'Const/volatile references stay rejected; pointer reference changes the pointer without touching a const pointee',async()=>{
      const count=assignments(),errors=[];
      for(const expression of ['const_reference','volatile_reference'])errors.push(await session.command('write_preview',{target:{kind:'variable',pane:'watch',expression},selection:{kind:'register'},input:{kind:'unsigned',text:'1'},context:await context()},false));
      assert.equal(assignments(),count);const result=await apply(await preview('pointer_reference',await evaluate('(unsigned long long)&cells.value')));assert.equal(await number('(unsigned long long)pointer_to_const'),await number('(unsigned long long)&cells.value'));assert.equal(await number('immutable'),11n);assert.equal(await number('volatile_value'),13n);return {errors,result};
    }))return;
    if(!await suite.test(ids[3],'Float reference preserves exact quiet NaN payload without executing target functions',async()=>{
      const draft=await preview('float_reference','12 00 c0 7f','bytes');assert.equal(draft.metadata.reference,true);const result=await apply(draft);assert.equal(result.observed.hex,'0x7fc00012');assert.equal(await number('*((unsigned int *)&single)'),0x7fc00012n);return result;
    }))return;
    if(!await suite.test(ids[4],'Actual DWARF 128-bit types preserve all high/low bits and signed range limits',async()=>{
      await session.command('watch_expand',{expression:'wide',path:[],expanded:true});await session.command('watch_expand',{expression:'wide',path:[0],expanded:true});const evidence=[];
      for(const value of ['0xfedcba98765432100123456789abcdef','0xffffffffffffffffffffffffffffffff']){
        const draft=await preview('wide',value,'unsigned',[0,1]);assert.equal(draft.metadata.scalar.bits,128);assert(!draft.literal.expression.includes('unsigned __int128'));evidence.push(await apply(draft));assert.equal(await number('((unsigned long long *)&wide.value)[0]'),BigInt(value)&((1n<<64n)-1n));assert.equal(await number('((unsigned long long *)&wide.value)[1]'),BigInt(value)>>64n);
      }
      for(const value of ['-170141183460469231731687303715884105728','170141183460469231731687303715884105727','-1']){const result=await apply(await preview('signed_wide',value,'signed'));assert.equal(BigInt(result.observed.hex),BigInt.asUintN(128,BigInt(value)));evidence.push(result);}
      const count=assignments();await session.command('write_preview',{target:{kind:'variable',pane:'watch',expression:'signed_wide'},selection:{kind:'register'},input:{kind:'unsigned',text:'170141183460469231731687303715884105728'},context:await context()},false);assert.equal(assignments(),count);assert.equal(await number('wide.before'),0x1122334455667788n);assert.equal(await number('wide.after'),0x8877665544332211n);return evidence;
    }))return;
    if(!await suite.test('WRITE-T-VAR-BITFIELD-DENIED','Bitfields without declared RAM byte order remain denied; target storage and neighbouring bits do not change',async()=>{
      await session.command('watch_expand',{expression:'bit_object',path:[],expanded:true});await session.command('watch_expand',{expression:'bit_object',path:[0],expanded:true});
      const word='*((unsigned int *)((char *)&bit_object+4))',before=await number(word),count=assignments(),errors=[];
      for(const index of [1,2])errors.push(await session.command('write_preview',{target:{kind:'variable',pane:'watch',expression:'bit_object',path:[0,index]},selection:{kind:'register'},input:{kind:'unsigned',text:'1'},context:await context()},false));
      assert(errors.every(e=>/bitfield/i.test(e)));assert.equal(assignments(),count);assert.equal(await number(word),before);assert.equal(await number('bit_object.before'),0x12345678n);assert.equal(await number('bit_object.after'),0x87654321n);return {before:before.toString(16),errors,byte_order_declared:false};
    }))return;
    if(!await suite.test(ids[5],'Local references remain bound to the selected caller frame and expire on frame switches',async()=>{
      await session.command('frame',{level:1});const draft=await session.command('write_preview',{target:{kind:'variable',pane:'locals',expression:'local_reference'},selection:{kind:'register'},input:{kind:'unsigned',text:'29'},context:await context()});assert.equal(draft.metadata.reference,true);const result=await apply(draft);assert.equal(await number('local'),29n);
      const stale=await session.command('write_preview',{target:{kind:'variable',pane:'locals',expression:'local_reference'},selection:{kind:'register'},input:{kind:'unsigned',text:'31'},context:await context()});const count=assignments();await session.command('frame',{level:0});assert.equal((await session.command('write_apply',{draft:stale.draft})).outcome,'not_sent');assert.equal(assignments(),count);assert.equal(await number('calls'),0n);assert.equal(await number('cells.before'),0x12345678n);assert.equal(await number('cells.after'),0x87654321n);assert(!session.logs('mi>').some(l=>l.text.includes('-data-write-memory')));return result;
    }))return;
  }catch(error){await suite.test('WRITE-T-VAR-REFERENCE-SETUP','Set up native C++ fixture',async()=>{throw error;});}
  finally{
    if(session)await suite.test(ids[6],'Close own process; preserve project and target function-call policy',async()=>{const policies=session.logs('mi>').filter(l=>l.text.includes('may-call-functions')).map(l=>l.text);await session.close();assert(policies.at(-1)?.includes(' on'));assert.equal(hash(project),originalHash);});
    for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'Native reference phase','Earlier native test failed');suite.finish();
  }
})();
