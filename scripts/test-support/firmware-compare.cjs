// Hardware-test helper. Never use GDB compare-sections here: OpenOCD's qCRC
// implementation can run a checksum loader in unbacked application RAM.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');

function readOnlySections(elf, {start, end}) {
  assert(Buffer.isBuffer(elf) && elf.length >= 52, 'Truncated ELF header');
  assert.equal(elf.subarray(0, 4).toString('hex'), '7f454c46', 'Invalid ELF magic');
  assert(elf[4] === 1 && elf[5] === 1 && elf[6] === 1, 'Expected little-endian ELF32');
  assert(elf.readUInt16LE(16) === 2 && elf.readUInt16LE(18) === 40, 'Expected ARM executable');
  assert(Number.isInteger(start) && Number.isInteger(end) && start >= 0 && end > start && end <= 0x100000000, 'Invalid firmware range');
  const offset = elf.readUInt32LE(32), stride = elf.readUInt16LE(46);
  const count = elf.readUInt16LE(48), namesIndex = elf.readUInt16LE(50);
  assert(stride >= 40 && count > 0 && namesIndex > 0 && namesIndex < count, 'Invalid section table');
  assert(offset >= 52 && offset + stride * count <= elf.length, 'Truncated section table');
  const slice = (offset, size) => {
    assert(offset + size <= elf.length, 'Truncated section contents');
    return elf.subarray(offset, offset + size);
  };
  const namesHeader = offset + stride * namesIndex;
  assert.equal(elf.readUInt32LE(namesHeader + 4), 3, 'Invalid section name table');
  const names = slice(elf.readUInt32LE(namesHeader + 16), elf.readUInt32LE(namesHeader + 20));
  const sections = [];
  for (let i = 1; i < count; i++) {
    const h = offset + stride * i, type = elf.readUInt32LE(h + 4), flags = elf.readUInt32LE(h + 8);
    const address = elf.readUInt32LE(h + 12), size = elf.readUInt32LE(h + 20);
    if (!(flags & 2) || (flags & 1) || type === 8 || size === 0) continue;
    const nameOffset = elf.readUInt32LE(h), nameEnd = names.indexOf(0, nameOffset);
    assert(nameOffset < names.length && nameEnd >= nameOffset, 'Invalid section name');
    const name = names.toString('utf8', nameOffset, nameEnd);
    // Refuse unexpected locations instead of reading peripherals/RAM or silently
    // accepting an incomplete firmware check. The caller owns the chip range.
    assert(address >= start && address + size <= end, `${name} outside allowed firmware range`);
    sections.push({name, address, bytes:slice(elf.readUInt32LE(h + 16), size)});
  }
  assert(sections.length > 0, 'No read-only firmware sections');
  return sections;
}

async function compareFirmware(command, elf, range) {
  const sections = readOnlySections(elf, range), evidence = [];
  for (const section of sections) {
    for (let offset = 0; offset < section.bytes.length; offset += 4096) {
      const expected = section.bytes.subarray(offset, offset + 4096);
      const address = `0x${(section.address + offset).toString(16)}`;
      const result = await command('memory_dump', {address, count:expected.length});
      assert(Array.isArray(result.bytes) && result.bytes.length === expected.length &&
        result.bytes.every(b => Number.isInteger(b) && b >= 0 && b <= 255), `${section.name}: invalid memory response at ${address}`);
      const actual = Buffer.from(result.bytes);
      if (!actual.equals(expected)) {
        const at = actual.findIndex((byte, i) => byte !== expected[i]);
        throw Error(`${section.name}: firmware mismatch at 0x${(section.address + offset + at).toString(16)}`);
      }
    }
    evidence.push({name:section.name, address:`0x${section.address.toString(16)}`, bytes:section.bytes.length,
      sha256:crypto.createHash('sha256').update(section.bytes).digest('hex')});
  }
  return evidence;
}

module.exports = {readOnlySections, compareFirmware};
