// Strict host-float MI failure model. Real GDB tests separately prove the API.
const scalar=require('./mock-variable-gdb.cjs');
const env=process.env,bits=Number(env.DEBUGTUI_TEST_FLOAT_WIDTH||32),bytes=bits/8;
const little=env.DEBUGTUI_TEST_FLOAT_ENDIAN!=='big';
const failures=new Set((env.DEBUGTUI_TEST_FLOAT_FAILURE||'').split('+'));
const applies=()=>inspections>1;
const fails=name=>failures.has(name)&&(!env.DEBUGTUI_TEST_FLOAT_APPLY_ONLY||applies());
let serial=0,inspections=0,calls='on',written=false;
let raw=bits===32?0x3f800000n:0x3ff0000000000000n;
const objects=new Map(),constants=new Map();
const hex=n=>'0x'+n.toString(16).padStart(bytes*2,'0');
const store=memory=>{
  const address=BigInt(env.DEBUGTUI_TEST_VARIABLE_ADDRESS||'0x100000004');
  for(let i=0;i<bytes;i++)memory.set(address+BigInt(i),Number((raw>>BigInt((little?i:bytes-1-i)*8))&255n));
};
module.exports.initialize=memory=>{scalar.initialize(memory);store(memory);};
module.exports.handle=(cmd,api)=>{
  const {done,error,running,memory}=api;
  if(cmd==='-gdb-set may-call-functions off'){calls='off';inspections++;}
  if(cmd==='-gdb-set may-call-functions on')calls='on';
  if(cmd==='-interpreter-exec console "ptype /r counter"'){
    process.stdout.write('~'+JSON.stringify('type = '+(bits===32?'float':'double')+'\n')+'\n');done();return true;
  }
  if(cmd==='-data-evaluate-expression "sizeof(counter)"'){done(`value="${bytes}"`);return true;}
  if(cmd==='-interpreter-exec console "show endian"'){
    const text=fails('endian')?'Byte order is unavailable':`The target endianness is set automatically (currently ${little?'little':'big'} endian)`;
    process.stdout.write('~'+JSON.stringify(text+'\n')+'\n');done();return true;
  }
  const consoleMatch=/^-interpreter-exec console (".*")$/.exec(cmd);
  if(consoleMatch){
    const python=JSON.parse(consoleMatch[1]);
    const create=/^python import gdb; assert gdb.lookup_type\('(float|double)'\).sizeof == (4|8); assert gdb.convenience_variable\('(__debugtui_literal_[0-9a-f]+_[0-9a-f]+)'\) is None; gdb.set_convenience_variable\('\3', gdb.Value\(bytes.fromhex\('([0-9a-f]+)'\), gdb.lookup_type\('\1'\)\)\)$/.exec(python);
    if(create){
      if(calls!=='off'||Number(create[2])!==bytes||create[4].length!==bytes*2){error('Invalid host buffer request');return true;}
      if(fails('python')||fails('type-size')||fails('collision')){error('GDB Python/type/name capability rejected');return true;}
      const octets=create[4].match(/../g),encoded=little?[...octets].reverse():octets;
      constants.set(create[3],fails('wrong-bits')?0n:BigInt('0x'+encoded.join('')));done();return true;
    }
    const cleanup=/^python gdb.set_convenience_variable\('(__debugtui_literal_[0-9a-f]+_[0-9a-f]+)', None\)$/.exec(python);
    if(cleanup){
      if(!constants.has(cleanup[1])){error('Unowned host constant');return true;}
      if(fails('cleanup')){error('Host constant cleanup unavailable');return true;}
      constants.delete(cleanup[1]);done();return true;
    }
  }
  const literal=expression=>{
    const match=/^\(__typeof__\(counter\)\)\((.+)\)$/.exec(expression);
    if(!match)return undefined;
    const value=match[1];
    if(/^\$__debugtui_literal_[0-9a-f]+_[0-9a-f]+$/.test(value))return constants.get(value.slice(1));
    const overflow=bits===32?'((float)(0x1p127 + 0x1p127))':'((double)(0x1p1023 + 0x1p1023))';
    const positive=bits===32?0x7f800000n:0x7ff0000000000000n;
    const negative=bits===32?0xff800000n:0xfff0000000000000n;
    if(value===overflow)return fails('change-expression')?0n:positive;
    if(value===`(-${overflow})`)return negative;
    if(value===`(-(${overflow} - ${overflow}))`||value===`(-(-(${overflow} - ${overflow})))`)return bits===32?0xffc00000n:0xfff8000000000000n;
    if(value==='1e0')return bits===32?0x3f800000n:0x3ff0000000000000n;
    return undefined;
  };
  if(cmd.startsWith('-var-create - * "')){
    const expression=JSON.parse(cmd.slice('-var-create - * '.length));
    const value=expression==='counter'?raw:literal(expression);
    if(value===undefined||calls!=='off'){error('Unsupported literal or target calls enabled');return true;}
    const name='float'+(++serial);objects.set(name,{value,root:expression==='counter'});
    done(`name="${name}",numchild="0",type="${bits===32?'float':'double'}",value="${hex(value)}"`);return true;
  }
  let match=/^-var-(show-attributes|info-path-expression|evaluate-expression -f hexadecimal|delete) "(float\d+)"$/.exec(cmd);
  if(match){
    const node=objects.get(match[2]);if(!node){error('No float object');return true;}
    if(match[1]==='delete'){
      objects.delete(match[2]);if(!node.root&&fails('literal-delete'))error('Literal object cleanup unavailable');else done('ndeleted="1"');return true;
    }
    if(match[1]==='show-attributes'){done('attr="editable"');return true;}
    if(match[1]==='info-path-expression'){done('path_expr="counter"');return true;}
    if(!node.root&&fails('literal-read')){error('Literal read unavailable');return true;}
    if(node.root&&written&&fails('readback')){error('Float readback unavailable');return true;}
    if(!node.root&&applies()&&fails('running-after-probe'))running();
    done(`value="${hex(node.root?raw:node.value)}"`);return true;
  }
  match=/^-var-assign "(float\d+)" (".*")$/.exec(cmd);
  if(match){
    const value=literal(JSON.parse(match[2]));
    if(!objects.get(match[1])?.root||calls!=='off'||value===undefined){error('Unsafe float assignment');return true;}
    written=true;raw=value;store(memory);
    if(fails('unknown'))error('Error after float assignment');else done(`value="${hex(raw)}"`);return true;
  }
  return scalar.handle(cmd,api);
};
