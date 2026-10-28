//! Observe the real execution frame, then let the auxiliary objfile unwind it.
//! Replaying an unwound return PC as frame zero loses caller lexical scopes.
use super::*;

pub(super) const NAMES: [&str; 17] = [
    "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr",
    "pc", "cpsr",
];

fn positions(names: &Value) -> [Option<usize>; 17] {
    let names = names.items();
    std::array::from_fn(|slot| {
        names.iter().position(|value| {
            let name = value.text();
            name == NAMES[slot] || matches!((slot, name), (13, "r13") | (14, "r14") | (15, "r15"))
        })
    })
}

fn decode(record: &Record, positions: &[Option<usize>; 17]) -> Result<[Option<u32>; 17], String> {
    let mut values = [None; 17];
    let mut seen = [false; 17];
    for value in record
        .data
        .field("register-values")
        .map(Value::items)
        .unwrap_or_default()
    {
        let number = value
            .string("number")
            .parse::<usize>()
            .map_err(|_| "Invalid observed register number")?;
        let slot = positions
            .iter()
            .position(|index| *index == Some(number))
            .ok_or("Unexpected observed register")?;
        if seen[slot] {
            return Err("Duplicate observed register".into());
        }
        seen[slot] = true;
        let raw = value.string("value");
        if matches!(
            raw.as_str(),
            "<unavailable>" | "<not saved>" | "<optimized out>"
        ) {
            continue;
        }
        values[slot] = Some(
            u32::try_from(literal_address(&raw)?).map_err(|_| "Observed register is not ARM32")?,
        );
    }
    Ok(values)
}

impl Engine {
    pub(super) fn image_frame_registers(
        &mut self,
        image: &Loaded,
    ) -> Result<([Option<u32>; 17], Vec<String>), String> {
        self.stopped()?;
        let thread = self.write_thread()?;
        if thread.is_empty() || !thread.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Current image frame needs a numeric GDB thread".into());
        }
        let state = self
            .snapshot
            .image_state
            .as_ref()
            .ok_or("No resident images")?;
        if state.execution.image.as_deref() != Some(image.id.as_str()) {
            return Err("Selected image differs from the physical execution image; inspect its saved Guest context instead".into());
        }
        let expected_pc = if self.snapshot.frame.level == 0 {
            self.snapshot.frame.address.clone()
        } else {
            self.snapshot
                .stack
                .iter()
                .find(|f| f.level == 0)
                .ok_or("Physical execution frame is unavailable")?
                .address
                .clone()
        };
        let names = self.mi("-data-list-register-names")?;
        let positions = positions(
            names
                .data
                .field("register-names")
                .ok_or("No observed register layout")?,
        );
        let indices = positions
            .iter()
            .flatten()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        if indices.is_empty() {
            return Err("No ARM32 integer registers in the physical GDB layout".into());
        }
        let record = self.mi(&format!(
            "-data-list-register-values --thread {thread} --frame 0 --skip-unavailable x {indices}"
        ))?;
        let values = decode(&record, &positions)?;
        let pc = values[15].ok_or("Current physical frame PC is unavailable")?;
        if u64::from(pc) & !1 != literal_address(&expected_pc)? & !1 {
            return Err("Physical frame PC changed during image expression capture".into());
        }
        let images = &self
            .snapshot
            .image_state
            .as_ref()
            .ok_or("No resident images")?
            .images;
        if crate::images::resolve(images, u64::from(pc), values[16])
            .image
            .as_deref()
            != Some(image.id.as_str())
        {
            return Err("Observed frame PC/mode does not identify the selected image".into());
        }
        let available = values
            .iter()
            .enumerate()
            .filter_map(|(slot, value)| value.map(|_| NAMES[slot].into()))
            .collect();
        Ok((values, available))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_frame_preserves_unavailable_values_aliases_and_nonconsecutive_layouts() {
        let names =
            mi::parse("1^done,register-names=[\"r0\",\"d0\",\"r13\",\"r14\",\"pc\",\"\",\"cpsr\"]")
                .unwrap()
                .unwrap();
        let layout = positions(names.data.field("register-names").unwrap());
        let record = mi::parse("2^done,register-values=[{number=\"0\",value=\"0x12345678\"},{number=\"2\",value=\"0x10026fc0\"},{number=\"3\",value=\"<unavailable>\"},{number=\"4\",value=\"0x10020164\"},{number=\"6\",value=\"0x600001d3\"}]").unwrap().unwrap();
        let values = decode(&record, &layout).unwrap();
        assert_eq!(values[0], Some(0x12345678));
        assert!(values[1..13].iter().all(Option::is_none));
        assert_eq!(values[13], Some(0x10026fc0));
        assert_eq!(values[14], None);
        assert_eq!(values[15], Some(0x10020164));
        assert_eq!(values[16], Some(0x600001d3));
        for reply in [
            "2^done,register-values=[{number=\"1\",value=\"0x1\"}]",
            "2^done,register-values=[{number=\"0\",value=\"0x1\"},{number=\"0\",value=\"0x2\"}]",
            "2^done,register-values=[{number=\"0\",value=\"0x100000000\"}]",
        ] {
            assert!(decode(&mi::parse(reply).unwrap().unwrap(), &layout).is_err());
        }
    }
}
