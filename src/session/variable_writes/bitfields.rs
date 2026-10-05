//! Bitfield inspection and verification under the existing service lease.
use super::*;
use crate::writes::BitfieldLayout;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct BitfieldMetadata {
    parent: String,
    parent_type: String,
    pub layout: BitfieldLayout,
    pub little_endian: bool,
    capture_bytes: usize,
}
impl Engine {
    pub(super) fn bitfield_metadata(
        &mut self,
        parent: &str,
        parent_type: &str,
        field: &str,
        bits: u16,
        declared_bytes: usize,
        context: &Context,
    ) -> Result<(Storage, BitfieldMetadata), String> {
        let parent_bytes = self.variable_size(parent)?;
        let layout = BitfieldLayout::parse(
            &self.variable_console(&format!("ptype /rod {parent}"))?,
            field,
            bits,
            parent_bytes,
        )?;
        if layout.declared_bytes != declared_bytes {
            return Err("DWARF bitfield type size differs from sizeof".into());
        }
        let storage = self.variable_storage(parent, parent_bytes, None, context)?;
        let base = super::super::memory::literal_address(
            storage
                .address
                .as_deref()
                .ok_or("Bitfield requires an actual parent RAM address")?,
        )?;
        let capture_bytes = layout.capture_bytes(base)?;
        if capture_bytes > 4096 {
            return Err("Bitfield parent/read span exceeds 4096 bytes".into());
        }
        let region = self
            .project
            .writes
            .resolve(base, capture_bytes, "", &context.core)?;
        if region.kind != MemoryKind::Ram
            || Some(&region.id) != storage.region.as_ref()
            || !region.widths.contains(&((declared_bytes * 8) as u16))
        {
            return Err("Bitfield write/read span is outside declared RAM widths".into());
        }
        let little_endian = region
            .little_endian
            .ok_or("Bitfield RAM byte order must be declared")?;
        let endian = self.variable_console("show endian")?;
        let actual = if endian.contains("currently little endian")
            || endian.contains("is set to little endian")
        {
            Some(true)
        } else if endian.contains("currently big endian") || endian.contains("is set to big endian")
        {
            Some(false)
        } else {
            None
        };
        if actual != Some(little_endian) {
            return Err(
                "GDB target byte order is unknown or differs from bitfield RAM policy".into(),
            );
        }
        Ok((
            storage,
            BitfieldMetadata {
                parent: parent.into(),
                parent_type: parent_type.into(),
                layout,
                little_endian,
                capture_bytes,
            },
        ))
    }
    pub(super) fn variable_value(
        &mut self,
        name: &str,
        metadata: &Metadata,
    ) -> Result<RawValue, String> {
        let raw = self.variable_raw(name, metadata.declared_bits)?;
        if metadata.bitfield.is_none() {
            return Ok(raw);
        }
        let n = raw.integer()?;
        let bits = metadata.scalar.bits;
        let mask = (1u128 << bits) - 1;
        let value = n & mask;
        let expected = if metadata.scalar.signed && value & (1 << (bits - 1)) != 0 {
            value | (((1u128 << metadata.declared_bits) - 1) & !mask)
        } else {
            value
        };
        if n != expected {
            return Err("GDB bitfield value has inconsistent extension or width".into());
        }
        RawValue::from_integer(value, bits)
    }
    pub(super) fn bitfield_bytes(
        &mut self,
        metadata: &Metadata,
        context: &Context,
    ) -> Result<Vec<u8>, String> {
        let field = metadata.bitfield.as_ref().ok_or("No bitfield layout")?;
        let dump = self.read_memory_dump(
            &json!({"address":metadata.address,"count":field.capture_bytes,"context":context}),
        )?;
        if dump["address"] != json!(metadata.address) {
            return Err("Bitfield parent read returned another address".into());
        }
        let bytes: Vec<u8> = serde_json::from_value(dump["bytes"].clone())
            .map_err(|e| format!("Bitfield bytes: {e}"))?;
        if bytes.len() != field.capture_bytes {
            return Err("Incomplete bitfield parent bytes".into());
        }
        Ok(bytes)
    }
    pub(super) fn bitfield_before(
        &mut self,
        metadata: &Metadata,
        name: &str,
        context: &Context,
    ) -> Result<Option<Vec<u8>>, String> {
        let Some(field) = metadata.bitfield.as_ref() else {
            return Ok(None);
        };
        let before = self.bitfield_bytes(metadata, context)?;
        if field.layout.extract(&before, field.little_endian)?
            != self.variable_value(name, metadata)?
        {
            return Err(
                "DWARF layout/raw memory and GDB field value disagree; no write sent".into(),
            );
        }
        Ok(Some(before))
    }
}
