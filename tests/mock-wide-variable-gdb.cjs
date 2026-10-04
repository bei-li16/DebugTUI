// Strict 128-bit failure model, separate from real C++/GDB proof.
const scalar=require('./mock-variable-gdb.cjs'),env=process.env;
let calls='on',inspections=0,serial=0,raw=1n,written=false;
const objects=new Map();
const fails=name=>env.DEBUGTUI_TEST_WIDE_FAILURE===name&&(!env.DEBUGTUI_TEST_WIDE_APPLY_ONLY||inspections>1);
const hex=n=>'0x'+n.toString(16).padStart(32,'0');
const store=memory=>{const address=BigInt(env.DEBUGTUI_TEST_VARIABLE_ADDRESS);for(let i=0;i<16;i++)memory.set(address+BigInt(i),Number((raw>>BigInt(i*8))&255n));};
const literal=expression=>{
  const parts=[...expression.matchAll(/0x([0-9a-f]+)/g)];if(parts.length!==2)return undefined;
  const [hi,lo]=parts.map(m=>m[1]);if(hi.length>16||lo.length>16)return undefined;
  const expected=`(__typeof__(counter))((((__typeof__(counter))0x${hi} << 64) | (__typeof__(counter))0x${lo}))`;
  return expression===expected?(BigInt('0x'+hi)<<64n)|BigInt('0x'+lo):undefined;
};
module.exports.initialize=memory=>{scalar.initialize(memory);store(memory);};
module.exports.handle=(cmd,api)=>{
  const {done,error,running,memory}=api;
  if(cmd==='-gdb-set may-call-functions off'){calls='off';inspections++;}
  if(cmd==='-gdb-set may-call-functions on')calls='on';
  if(cmd==='-data-evaluate-expression "sizeof(counter)"'){done('value="16"');return true;}
  if(cmd==='-data-evaluate-expression "counter"'){done(`value="${raw}"`);return true;}
  if(cmd.startsWith('-var-create - * "')){
    const expression=JSON.parse(cmd.slice('-var-create - * '.length));const value=expression==='counter'?raw:literal(expression);
    if(value===undefined||calls!=='off'){error('Unsupported wide literal or target calls enabled');return true;}
    if(expression!=='counter'&&fails('unsupported')){error('128-bit host evaluation unavailable');return true;}
    const name='wide'+(++serial);objects.set(name,{value,root:expression==='counter'});done(`name="${name}",numchild="0",type="__int128 unsigned",value="${value}"`);return true;
  }
  let match=/^-var-(show-attributes|info-path-expression|evaluate-expression -f hexadecimal|delete) "(wide\d+)"$/.exec(cmd);
  if(match){
    const node=objects.get(match[2]);if(!node){error('No wide variable object');return true;}
    if(match[1]==='delete'){objects.delete(match[2]);if(!node.root&&fails('cleanup'))error('Wide literal cleanup unavailable');else done('ndeleted="1"');return true;}
    if(match[1]==='show-attributes'){done('attr="editable"');return true;}
    if(match[1]==='info-path-expression'){done('path_expr="counter"');return true;}
    if(!node.root&&fails('running'))running();
    if(node.root&&written&&fails('readback')){error('Wide readback unavailable');return true;}
    const value=node.root?raw:fails('truncate')?BigInt.asUintN(64,node.value):node.value;
    done(`value="${hex(value)}"`);return true;
  }
  match=/^-var-assign "(wide\d+)" (".*")$/.exec(cmd);
  if(match){
    const value=literal(JSON.parse(match[2]));if(!objects.get(match[1])?.root||calls!=='off'||value===undefined){error('Unsafe wide assignment');return true;}
    written=true;raw=value;store(memory);if(fails('unknown'))error('Error after wide assignment');else done(`value="${value}"`);return true;
  }
  return scalar.handle(cmd,api);
};
