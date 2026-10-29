const {test} = require('node:test');
const assert = require('node:assert/strict');
const {readOnlySections, compareFirmware} = require('./firmware-compare.cjs');
const range = {start:0x08000000,end:0x08200000};

function fixture() {
  const names = Buffer.from('\0.text\0.data\0.bss\0.debug_info\0.shstrtab\0');
  const elf = Buffer.alloc(6000);
  Buffer.from('7f454c46010101','hex').copy(elf);
  elf.writeUInt16LE(2,16); elf.writeUInt16LE(40,18);
  elf.writeUInt32LE(52,32); elf.writeUInt16LE(40,46); elf.writeUInt16LE(6,48); elf.writeUInt16LE(5,50);
  function section(i, name, type, flags, address, offset, size) {
    const h = 52 + i * 40;
    [names.indexOf(Buffer.from(name)), type, flags, address, offset, size].forEach((v,j)=>elf.writeUInt32LE(v,h+j*4));
  }
  section(1,'.text',1,6,range.start,1024,4100);
  section(2,'.data',1,3,0x20000000,5200,4);
  section(3,'.bss',8,3,0x20000004,0,100);
  section(4,'.debug_info',1,0,0,5300,8);
  section(5,'.shstrtab',3,0,0,512,names.length);
  names.copy(elf,512); elf.fill(0xa5,1024,5124);
  return elf;
}

test('reads every read-only firmware byte in bounded chunks without target CRC or execution',async()=>{
  const elf = fixture(), calls = [];
  const evidence = await compareFirmware(async(method, params)=>{
    calls.push({method,...params});
    assert.equal(method,'memory_dump');
    const offset = Number(params.address)-range.start;
    return {bytes:[...elf.subarray(1024+offset,1024+offset+params.count)]};
  },elf,range);
  assert.deepEqual(calls,[{method:'memory_dump',address:'0x8000000',count:4096},{method:'memory_dump',address:'0x8001000',count:4}]);
  assert.equal(evidence.length,1); assert.equal(evidence[0].bytes,4100);
  assert.match(evidence[0].sha256,/^[a-f0-9]{64}$/);
});

test('detects byte mismatches, short replies and invalid bytes',async()=>{
  await assert.rejects(compareFirmware(async(_,p)=>({bytes:Array(p.count).fill(0)}),fixture(),range),/mismatch at 0x8000000/);
  await assert.rejects(compareFirmware(async()=>({bytes:[]}),fixture(),range),/invalid memory response/);
  await assert.rejects(compareFirmware(async(_,p)=>({bytes:Array(p.count).fill(256)}),fixture(),range),/invalid memory response/);
});

test('rejects malformed/unsupported ELF before any hardware access',async()=>{
  for (const corrupt of [
    b=>b.subarray(0,20), b=>{b[4]=2;return b;},
    b=>{b.writeUInt32LE(5990,32);return b;},
    b=>{b.writeUInt32LE(5990,52+40+16);return b;},
    b=>{b.writeUInt32LE(0x40000000,52+40+12);return b;},
    b=>{b.writeUInt32LE(9999,52+40);return b;},
    b=>{b.writeUInt32LE(3,52+40+8);return b;}
  ]) {
    let calls=0;
    await assert.rejects(compareFirmware(async()=>{calls++;},corrupt(fixture()),range));
    assert.equal(calls,0);
  }
});

test('includes allocated read-only ARM exception tables',()=>{
  const elf=fixture(); elf.writeUInt32LE(0x70000001,52+40+4);
  assert.equal(readOnlySections(elf,range)[0].bytes.length,4100);
});
