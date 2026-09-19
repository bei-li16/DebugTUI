// Native GDB integration; Node and GCC are development-only test dependencies.
const {spawn, execFileSync} = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const assert = require('node:assert/strict');
const root = path.dirname(__dirname);
const binary = path.resolve(process.argv[2] || path.join(root, 'target/release/debugtui.exe'));
const output = path.join(root, 'artifacts', `watch-tree-${Date.now()}`);
fs.mkdirSync(output, {recursive:true});
const quote = p => JSON.stringify(p.replaceAll('\\','/'));
fs.writeFileSync(path.join(output,'sample.c'), `
#include <stdint.h>
struct Pair { uint32_t value; uint32_t enabled; };
struct Outer { struct Pair pair; uint32_t items[70]; struct Pair *ptr; };
struct Pair counter_pair = {41, 1};
struct Pair *counter_ptr = &counter_pair;
struct Outer outer = {{53, 2}, {11, 12, 13}, &counter_pair};
struct Link { int value; struct Link *next; } link = {5, &link};
volatile uint32_t counter = 17;
static volatile uint32_t counter_static = 23;
uint32_t large_array[300] = {19};
void counter_function(void) { counter++; }
int main(void) {
    for (;;) { counter += 0; }
}
`);
execFileSync(process.env.DEBUGTUI_TEST_CC || 'C:/MinGW/bin/gcc.exe', ['-g','-O0',path.join(output,'sample.c'),'-o',path.join(output,'sample.exe')]);
const gdb = process.env.DEBUGTUI_TEST_GDB || 'C:/MinGW/bin/gdb.exe';
fs.writeFileSync(path.join(output,'project.toml'), `version=2
[gdb]
executable=${quote(gdb)}
[target]
mode="local"
[program]
elf=${quote(path.join(output,'sample.exe'))}
[session]
log_dir=${quote(output)}
on_exit="disconnect"
`);
const child = spawn(binary, ['--project',path.join(output,'project.toml'),'--headless','--stdio'], {windowsHide:true});
const events = fs.createWriteStream(path.join(output,'events.jsonl'));
const pending = new Map(), mi = [], report = [], created = new Set(), deleted = new Set();
let nextId = 1;
const exit = new Promise(resolve => child.once('exit', resolve));
readline.createInterface({input:child.stdout}).on('line', line => {
  events.write(line+'\n');
  const event = JSON.parse(line);
  if(event.event==='log' && event.channel==='mi>') {
    mi.push(event.text);
    const name=event.text.match(/-var-delete "(var\d+)"/);if(name) deleted.add(name[1]);
  }
  if(event.event==='log' && event.channel==='mi<') {
    const match=event.text.match(/^\d+\^done (\[.*\])$/);
    if(match) {
      const name=JSON.parse(match[1]).find(([key])=>key==='name')?.[1];
      if(typeof name==='string' && /^var\d+$/.test(name)) created.add(name);
    }
  }
  if(event.event==='response') pending.get(event.id)?.(event);
});
child.stderr.on('data', data => fs.appendFileSync(path.join(output,'stderr.log'), data));
async function cmd(method, params={}, ok=true) {
  const id = nextId++;
  const event = await new Promise((resolve,reject) => {
    const timer=setTimeout(()=>{pending.delete(id);reject(Error(`Timeout: ${method}`));},25000);
    pending.set(id, event=>{clearTimeout(timer);pending.delete(id);resolve(event);});
    child.stdin.write(JSON.stringify({id,method,params})+'\n');
  });
  assert.equal(event.ok,ok,`${method}: ${event.error}`);
  return event.result;
}
async function watch(name) { return (await cmd('status')).watches.find(v=>v.name===name); }
async function expand(expression, path=[], extra={}) { await cmd('watch_expand',{expression,path,expanded:true,...extra});return watch(expression); }
const contains = (values,name) => assert(values.includes(name), `Missing completion: ${name}, got ${JSON.stringify(values)}`);
(async()=>{
  try {
    await cmd('connect');
    const names=(await cmd('complete',{text:'counter',expression:true})).matches;
    for(const name of ['counter','counter_pair','counter_ptr','counter_static']) contains(names,name);
    assert(!names.includes('counter_function'));
    for(const [text, expected] of [['counter_pair.v','counter_pair.value'],['counter_ptr->e','counter_ptr->enabled'],['outer.pair.v','outer.pair.value'],['((struct Pair *)counter_ptr)->v','((struct Pair *)counter_ptr)->value']]) {
      contains((await cmd('complete',{text,expression:true})).matches,expected);
    }
    report.push('Watch completion: global/static symbols, nested members, pointer members and cast expressions');
    await cmd('break',{location:'main'});await cmd('run');await cmd('wait_stopped');
    const stopped=await cmd('status');
    const start=mi.length;
    await cmd('watch',{expression:'outer'});
    assert.equal((await watch('outer')).tree.expanded,false);
    assert.equal((await watch('outer')).tree.child_count,3);
    assert(!mi.slice(start).some(s=>s.includes('-var-list-children')), 'Collapsed Watch must not query children');
    let v=await expand('outer');
    assert.deepEqual(v.tree.children.map(c=>c.name),['pair','items','ptr']);
    v=await expand('outer',[0]);
    assert.equal(v.tree.children[0].tree.children[0].value,'53');
    v=await expand('outer',[1]);
    assert.equal(v.tree.children[1].tree.children.length,32);
    assert.equal(v.tree.children[1].tree.has_more,true);
    v=await expand('outer',[1],{more:true});
    assert.equal(v.tree.children[1].tree.children.length,64);
    v=await expand('outer',[1],{more:true});
    assert.equal(v.tree.children[1].tree.children.length,70);
    assert.equal(v.tree.children[1].tree.has_more,false);
    assert.equal(v.tree.children[1].tree.children[2].value,'13');
    const collapseStart=mi.length;
    await cmd('watch_expand',{expression:'outer',path:[1],expanded:false});
    assert.equal(mi.length,collapseStart,'Collapse must not access the target');
    report.push('Lazy nested structure expansion, array paging (32/64/70), collapse with zero MI commands');
    await cmd('console',{command:'set variable outer.pair.value = 91'});
    v=await watch('outer');
    assert.equal(v.tree.children[0].tree.children[0].value,'91');
    assert.equal(v.tree.children[0].tree.children[0].changed,true);
    assert.equal(v.tree.children[1].tree.expanded,false);
    report.push('Expanded members refresh and highlight changes; collapsed branches stay collapsed');
    const structAddr=(await cmd('evaluate',{expression:'&counter_pair'})).value.match(/0x[\da-f]+/i)[0];
    const intAddr=(await cmd('evaluate',{expression:'&counter'})).value.match(/0x[\da-f]+/i)[0];
    for(const expression of [`(struct Pair *)(${structAddr})`,`*(struct Pair *)(${structAddr})`]) {
      await cmd('watch',{expression});
      const value=await expand(expression);
      assert.equal(value.tree.children.find(c=>c.name==='value').value,'41');
    }
    const pointer=`(uint32_t *)(${intAddr})`, scalar=`*(uint32_t *)(${intAddr})`;
    await cmd('watch',{expression:pointer});
    assert.equal((await expand(pointer)).tree.children[0].value,'17');
    await cmd('watch',{expression:scalar});
    assert.equal((await watch(scalar)).value,'17');
    await cmd('watch',{expression:'*(uint32_t *)0x1'});
    assert.equal((await watch('*(uint32_t *)0x1')).error,true,'Invalid memory must show an error');
    await cmd('watch',{expression:'(struct Pair *)0x1'});
    const invalid=await expand('(struct Pair *)0x1');
    assert(invalid.error || invalid.tree.children.every(v=>v.error), 'Unreadable structure children must show errors');
    await cmd('watch',{expression:'*(struct NoSuchType *)0x1'});
    assert.equal((await watch('*(struct NoSuchType *)0x1')).error,true);
    assert.equal((await cmd('status')).state,'STOPPED');
    assert.equal((await cmd('status')).frame.address,stopped.frame.address);
    report.push('Address casts: structure pointers/dereference, uint32_t pointers/dereference, invalid memory/type errors');
    await cmd('watch_expand',{expression:'outer',path:[99],expanded:true},false);
    await cmd('watch',{expression:'large_array'});
    for(let page=0;page<8;page++) await expand('large_array',[],{more:page>0});
    v=await watch('large_array');assert.equal(v.tree.children.length,255);assert(v.tree.limited);
    await cmd('watch_expand',{expression:'large_array',expanded:true,more:true},false);
    await cmd('watch_expand',{expression:'large_array',expanded:false});
    await expand('large_array');
    assert.equal((await watch('large_array')).tree.children.length,32,'Reopening a collapsed tree starts a fresh bounded page');
    await cmd('watch',{expression:'link'});
    for(let depth=0;depth<=8;depth++) await expand('link',Array(depth).fill(1));
    v=await watch('link');
    for(let depth=0;depth<8;depth++) v=v.tree.children[1];
    assert(v.tree.limited && v.tree.has_more);
    await cmd('watch_expand',{expression:'link',path:Array(8).fill(1),expanded:true},false);
    assert(created.size>0);
    assert.deepEqual(created,deleted,'Every successfully created variable object must be deleted, including failed reads');
    await cmd('delete_break',{number:''});await cmd('continue');
    await cmd('watch_expand',{expression:'outer',path:[0],expanded:true},false);
    const removalStart=mi.length;
    await cmd('watch_expand',{expression:'outer',expanded:false});
    for(const value of (await cmd('status')).watches) await cmd('unwatch',{expression:value.name});
    assert.equal(mi.length,removalStart);
    await cmd('pause');assert.deepEqual((await cmd('status')).watches,[]);
    await cmd('watch',{expression:'outer'});assert.equal((await watch('outer')).tree.expanded,false);
    report.push('Bounded expansion, stale path rejection, no target access on running collapse/delete, no resurrected trees');
    await cmd('quit');assert.equal(await exit,0);
    fs.writeFileSync(path.join(output,'verification.json'),JSON.stringify({passed:true,binary,commands:nextId-1,report},null,2));
    console.log(`PASS Watch tree: ${output}`);
  } finally {
    if(child.exitCode===null) {
      child.stdin.end(JSON.stringify({id:nextId++,method:'quit'})+'\n');
      const timer=setTimeout(()=>child.kill(),10000);await exit;clearTimeout(timer);
    }
    events.end();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
