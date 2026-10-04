// Actual native GDB/GCC typed writes. This test never connects to a board.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const {root,hash,outputDirectory,Cases,Session}=require('./test-support/session.cjs');
const out=outputDirectory('variable-write-native');
const binary=path.resolve(process.argv[2]||path.join(root,'target/debug/debugtui.exe'));
const cc=process.env.DEBUGTUI_TEST_CC||'C:/MinGW/bin/gcc.exe',gdb=process.env.DEBUGTUI_TEST_GDB||'C:/MinGW/bin/gdb.exe';
const suite=new Cases(out,{layer:'native GCC/GDB local process; no board',board_tests_executed:false,binary,binary_sha256:hash(binary),gdb,cc});
const ids=['WRITE-T-VAR-CANCEL','WRITE-T-VAR-SCALARS','WRITE-T-VAR-MEMBERS','WRITE-T-VAR-CONST','WRITE-T-VAR-POINTER','WRITE-T-VAR-FLOAT','WRITE-T-VAR-LOCALS','WRITE-T-VAR-CLEANUP'];
const source=path.join(out,'sample.c'),exe=path.join(out,'sample.exe'),project=path.join(out,'project.toml');
fs.writeFileSync(source,`
#include <stdint.h>
typedef const uint32_t ConstAlias;
uint32_t value=7, neighbours[2]={0x11223344,0x55667788};
int32_t signed_value=-2;
uint64_t wide_value=1;
float float_value=1.5f;
double double_value=2.5;
const uint32_t constant=9;
ConstAlias alias_constant=10;
uint32_t *const fixed_pointer=&value;
const uint32_t *pointer_to_const=&constant;
struct Pair { uint32_t member; int items[3]; } object={12,{3,4,5}};
const struct Pair const_object={19,{20,21,22}};
struct Pair *pair_pointer=&object;
unsigned calls=0;
int side_effect(void){calls++;return 42;}
void checkpoint(void){calls+=0;}
void fixture(int argument){struct Pair local_object={27,{10,11,12}};checkpoint();value+=(local_object.member+argument)*0;}
int main(void){fixture(42);return 0;}
`);
execFileSync(cc,['-g','-O0',source,'-o',exe],{windowsHide:true});
const quote=x=>JSON.stringify(x.replaceAll('\\','/'));
const watches=['value','signed_value','wide_value','float_value','double_value','constant','alias_constant','fixed_pointer','pointer_to_const','object','const_object','pair_pointer'];
fs.writeFileSync(project,`version=2\nwatch=${JSON.stringify(watches)}\n[[breakpoints]]\nlocation='checkpoint'\nkind='code'\n[gdb]\nexecutable=${quote(gdb)}\n[target]\nmode='local'\n[program]\nelf=${quote(exe)}\n[session]\non_exit='disconnect'\n[[writes.regions]]\nid='native-fixture-ram'\nkind='ram'\nstart='0x1'\nend='0xffffffffffffffff'\nwidths=[8,16,32,64,128]\nscope='core'\n`);
const originalHash=hash(project);let session;
const context=async()=>(await session.command('registers_list')).context;
const request=async(expression,text,kind='unsigned',extra={})=>session.command('write_preview',{target:{kind:'variable',pane:'watch',expression,path:[],...extra},selection:{kind:'register'},input:{kind,text},context:await context()});
const apply=async(draft)=>{const result=await session.command('write_apply',{draft:draft.draft});assert.equal(result.outcome,'verified',JSON.stringify(result));return result;};
const evaluate=async expression=>(await session.command('evaluate',{expression})).value;
const readNumber=async expression=>BigInt((await evaluate(expression)).split(' ')[0]);
(async()=>{
  try{
    session=new Session(binary,project,out);await session.command('connect');await session.command('run');await session.command('wait_stopped');
    if(!await suite.test('WRITE-T-VAR-CANCEL','Preview/cancel performs no assignment and restores GDB function-call policy',async()=>{
      const before=session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length;
      const draft=await request('value','42');assert.equal(draft.metadata.scalar.bits,32);assert.equal(draft.metadata.type_name,'unsigned int');
      const cancelled=await session.command('write_cancel',{draft:draft.draft});assert.equal(cancelled.cancelled,true);assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');
      assert.equal(await readNumber('value'),7n);assert.equal(session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length,before);
      const policy=session.logs('mi>').filter(l=>l.text.includes('may-call-functions')).map(l=>l.text);assert(policy.some(l=>l.includes(' off'))&&policy.at(-1).includes(' on'));return {draft,cancelled,policy};
    }))return;
    if(!await suite.test('WRITE-T-VAR-SCALARS','Exact unsigned/signed/64-bit writes and independent reads preserve neighbouring variables',async()=>{
      const evidence=[];
      for(const [name,text,kind,expected]of [['value','4294967295','unsigned',4294967295n],['signed_value','-2147483648','signed',-2147483648n],['wide_value','18446744073709551615','unsigned',18446744073709551615n]]){
        const draft=await request(name,text,kind);evidence.push(await apply(draft));assert.equal(await readNumber(name),expected);assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');
      }
      assert.equal(await readNumber('neighbours[0]'),0x11223344n);assert.equal(await readNumber('neighbours[1]'),0x55667788n);return evidence;
    }))return;
    if(!await suite.test('WRITE-T-VAR-MEMBERS','Native struct and array member paths select exactly one typed object',async()=>{
      await session.command('watch_expand',{expression:'object',path:[],expanded:true});await session.command('watch_expand',{expression:'object',path:[1],expanded:true});
      const member=await request('object','33','unsigned',{path:[0]});assert.equal(member.metadata.expression,'(object).member');await apply(member);assert.equal(await readNumber('object.member'),33n);
      const element=await request('object','-17','signed',{path:[1,1]});await apply(element);assert.equal(await readNumber('object.items[1]'),-17n);assert.equal(await readNumber('object.items[0]'),3n);assert.equal(await readNumber('object.items[2]'),5n);return {member,element};
    }))return;
    if(!await suite.test('WRITE-T-VAR-CONST','Reject const, const aliases, const pointer, aggregate, range overflow and function expressions before assignment',async()=>{
      const before=session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length;const errors=[];
      for(const [name,text]of [['constant','1'],['alias_constant','1'],['fixed_pointer','1'],['signed_value','2147483648'],['object','1'],['side_effect()','1']]){
        errors.push(await session.command('write_preview',{target:{kind:'variable',pane:'watch',expression:name,path:[]},selection:{kind:'register'},input:{kind:'unsigned',text},context:await context()},false));
      }
      errors.push(await session.command('write_preview',{target:{kind:'variable',pane:'watch',expression:'const_object',path:[0]},selection:{kind:'register'},input:{kind:'unsigned',text:'1'},context:await context()},false));
      assert.equal(session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length,before);assert.equal(await readNumber('calls'),0n);assert.equal(await readNumber('constant'),9n);return errors;
    }))return;
    if(!await suite.test('WRITE-T-VAR-POINTER','Pointer-to-const value can change without writing the const pointee',async()=>{
      const address=await evaluate('(unsigned long long)&constant');const result=await apply(await request('pointer_to_const',address));assert.equal(await readNumber('constant'),9n);
      await apply(await request('pair_pointer',await evaluate('(unsigned long long)&object')));assert.equal(await readNumber('object.member'),33n);return result;
    }))return;
    if(!await suite.test('WRITE-T-VAR-FLOAT','Finite typed float/double writes and exact negative-zero readback',async()=>{
      const negative=await apply(await request('float_value','-0','float'));assert.equal(negative.observed.hex,'0x80000000');
      const regular=await apply(await request('double_value','3.25','float'));assert.equal(await evaluate('double_value'),'3.25');return {negative,regular};
    }))return;
    if(!await suite.test('WRITE-T-VAR-LOCALS','Caller-frame Locals struct/array members and argument remain bound to the selected frame',async()=>{
      await session.command('frame',{level:1});const status=await session.command('status');assert(status.locals.some(v=>v.name==='local_object'));
      await session.command('local_expand',{expression:'local_object',path:[],expanded:true});await session.command('local_expand',{expression:'local_object',path:[1],expanded:true});
      const listed=await session.command('status');assert.equal(listed.locals.find(v=>v.name==='local_object').tree.children[1].tree.children.length,3);
      const draft=await session.command('write_preview',{target:{kind:'variable',pane:'locals',expression:'local_object',path:[1,2]},selection:{kind:'register'},input:{kind:'signed',text:'-19'},context:await context()});const result=await apply(draft);assert.equal(await readNumber('local_object.items[2]'),-19n);
      const stale=await session.command('write_preview',{target:{kind:'variable',pane:'locals',expression:'argument'},selection:{kind:'register'},input:{kind:'signed',text:'13'},context:await context()});await session.command('frame',{level:0});assert.equal((await session.command('write_apply',{draft:stale.draft})).outcome,'not_sent');return result;
    }))return;
  }catch(error){await suite.test('WRITE-T-VAR-SETUP','Native typed writer session setup',async()=>{throw error;});}
  finally{
    if(session)await suite.test('WRITE-T-VAR-CLEANUP','Close only the native fixture; preserve original project',async()=>{await session.close();assert.equal(hash(project),originalHash);});
    for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'Native variable writer phase','Earlier native test failed');suite.finish();
  }
})();
