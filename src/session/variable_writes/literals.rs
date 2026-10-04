//! Exact GDB-host float constants. This module never assigns inferior memory.
use super::*;

impl Engine {
    pub(super) fn with_exact_float_literal<T>(
        &mut self,
        metadata: &Metadata,
        raw: &RawValue,
        body: impl FnOnce(&mut Self, &str) -> Result<T, String>,
    ) -> Result<T, String> {
        let type_name = match raw.bits {
            32 => "float",
            64 => "double",
            _ => return Err("Exact float literal requires 32 or 64 bits".into()),
        };
        if self.console_capture.is_some() {
            return Err("Nested endian capture is unavailable".into());
        }
        self.console_capture = Some(String::new());
        let queried = self.console("show endian");
        let endian = self.console_capture.take().unwrap_or_default();
        queried?;
        let little = match (
            endian.contains("little endian"),
            endian.contains("big endian"),
        ) {
            (true, false) => true,
            (false, true) => false,
            _ => {
                return Err(
                    "Cannot determine current GDB byte order for an exact float literal".into(),
                );
            }
        };
        let bytes = usize::from(raw.bits / 8);
        let number = raw.integer()?;
        let encoded = if little {
            number.to_le_bytes()[..bytes].to_vec()
        } else {
            number.to_be_bytes()[16 - bytes..].to_vec()
        };
        let hex = encoded
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let name = format!(
            "__debugtui_literal_{:x}_{:x}",
            self.register_session,
            NEXT_DRAFT.fetch_add(1, Ordering::Relaxed)
        );
        // Built-in types, bounded hex and owned names only. Buffer construction
        // creates a nonaddressable GDB value, not an inferior allocation/call.
        self.console(&format!("python import gdb; assert gdb.lookup_type('{type_name}').sizeof == {bytes}; assert gdb.convenience_variable('{name}') is None; gdb.set_convenience_variable('{name}', gdb.Value(bytes.fromhex('{hex}'), gdb.lookup_type('{type_name}')))"))
            .map_err(|e| format!("Exact float literal requires the verified GDB Python buffer API; no write sent: {e}"))?;
        let expression = format!("${name}");
        let result = (|| {
            let observed = self.check_variable_literal(metadata, raw, &expression)?;
            if observed != *raw {
                return Err(format!(
                    "GDB exact literal mismatch: requested {}, observed {}; no write sent",
                    raw.hex, observed.hex
                ));
            }
            body(self, &expression)
        })();
        let cleanup = self.console(&format!(
            "python gdb.set_convenience_variable('{name}', None)"
        ));
        match (result, cleanup) {
            (Ok(value), Ok(_)) => Ok(value),
            (Err(e), Ok(_)) => Err(e),
            (Err(e), Err(cleanup)) => Err(format!(
                "{e}; exact numeric literal cleanup failed: {cleanup}"
            )),
            (Ok(_), Err(e)) => Err(format!("Exact numeric literal cleanup failed: {e}")),
        }
    }
}
