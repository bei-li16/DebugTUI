//! Description evidence is separate from observed target identity and read provenance.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub document: String,
    pub version: String,
    #[serde(default)]
    pub number: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualSource {
    #[serde(default)]
    pub document: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub number: String,
    pub section: String,
    /// One-based physical PDF page, rather than an ambiguous printed page label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Pinned source headers use one-based lines instead of fabricated PDF pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    #[default]
    Unknown,
    Low,
    Medium,
    High,
    Verified,
}
impl Confidence {
    pub fn is_unknown(&self) -> bool {
        *self == Self::Unknown
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Verified => "verified",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HardwareVerification {
    pub date: String,
    pub board: String,
    pub firmware: String,
    pub report: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefinitionOrigin {
    pub declared_in: String,
    /// Root to declaring file, including every inheritance step.
    pub inheritance: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<String>,
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
fn url_valid(url: &Option<String>) -> bool {
    url.as_ref().is_none_or(|value| {
        nonempty(value) && (value.starts_with("https://") || value.starts_with("http://"))
    })
}
fn date_valid(value: &str) -> bool {
    if value.len() != 10
        || !value.bytes().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return false;
    }
    let year: u32 = value[..4].parse().unwrap_or(0);
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..].parse().unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    year > 0 && day > 0 && day <= days
}
impl Document {
    pub(super) fn validate(&self) -> Result<(), String> {
        if !nonempty(&self.document) || !nonempty(&self.version) || !url_valid(&self.url) {
            return Err(
                "Description document requires a name, version and valid source URL".into(),
            );
        }
        Ok(())
    }
}
impl ManualSource {
    pub(super) fn inherit_document(&mut self, meta: Option<&Document>) {
        if let Some(meta) = meta
            && self.document.is_empty()
        {
            self.document = meta.document.clone();
            if self.version.is_empty() {
                self.version = meta.version.clone();
            }
            if self.number.is_empty() {
                self.number = meta.number.clone();
            }
            if self.url.is_none() {
                self.url = meta.url.clone();
            }
        }
    }
    fn validate(&self) -> Result<(), String> {
        if !nonempty(&self.document)
            || !nonempty(&self.version)
            || !nonempty(&self.section)
            || (self.page.is_none() && self.line.is_none())
            || self.page == Some(0)
            || self.line == Some(0)
            || !url_valid(&self.url)
        {
            return Err("Source requires document/version/section and a positive PDF page or pinned header line".into());
        }
        Ok(())
    }
}

impl Register {
    pub(super) fn validate_metadata(&self, version: u32) -> Result<(), String> {
        if (version == 2 || self.confidence != Confidence::Unknown) && self.source.is_none() {
            return Err(format!(
                "Register {}: version 2 or declared confidence requires source",
                self.id
            ));
        }
        if let Some(source) = &self.source {
            source
                .validate()
                .map_err(|error| format!("Register {}: {error}", self.id))?;
        }
        if let Some(reset) = self.reset {
            RawValue::parse(&reset.to_string(), self.bits)
                .map_err(|error| format!("Register {} reset: {error}", self.id))?;
        }
        match (&self.verification, self.confidence) {
            (Some(proof), Confidence::Verified)
                if date_valid(&proof.date)
                    && nonempty(&proof.board)
                    && nonempty(&proof.firmware)
                    && nonempty(&proof.report) => {}
            (None, Confidence::Verified) | (Some(_), _) => {
                return Err(format!(
                    "Register {}: verified confidence requires dated board/firmware/report evidence; other confidence levels cannot claim hardware verification",
                    self.id
                ));
            }
            _ => {}
        }
        Ok(())
    }
    pub fn definition_details(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Definition confidence: {}", self.confidence.label()),
            format!(
                "Reset: {}",
                self.reset
                    .map(|value| format!("0x{value:x}"))
                    .unwrap_or_else(|| "Unknown; no verified reset definition".into())
            ),
        ];
        if let Some(origin) = &self.definition_origin {
            lines.push(format!("Definition file: {}", origin.declared_in));
            if origin.inheritance.len() > 1 {
                lines.push(format!("Inheritance: {}", origin.inheritance.join(" -> ")));
            }
            if let Some(parent) = &origin.overrides {
                lines.push(format!("Explicitly overrides: {parent}"));
            }
        }
        if let Some(source) = &self.source {
            lines.push(format!(
                "Manual source: {} · {} · {} · {}",
                source.document, source.version, source.number, source.section
            ));
            if let Some(page) = source.page {
                lines.push(format!("PDF page: {page}"));
            }
            if let Some(line) = source.line {
                lines.push(format!("Source line: {line}"));
            }
            if let Some(url) = &source.url {
                lines.push(format!("Source URL: {url}"));
            }
        } else {
            lines.push("Manual source: Unknown; legacy definition without source metadata".into());
        }
        if self.fields_missing {
            lines.push("Fields: incomplete; unspecified bits remain unknown".into());
        }
        if let Some(proof) = &self.verification {
            lines.push(format!(
                "Declared hardware verification: {} · {} · {} · {}",
                proof.date, proof.board, proof.firmware, proof.report
            ));
        }
        lines
    }
}

impl Catalogue {
    /// Diagnostics are kept outside the persisted definition schema.
    pub fn definition_origins(&self) -> BTreeMap<&str, &DefinitionOrigin> {
        self.registers
            .iter()
            .filter_map(|register| {
                register
                    .definition_origin
                    .as_ref()
                    .map(|origin| (register.id.as_str(), origin))
            })
            .collect()
    }
    pub fn confidence_summary(&self) -> String {
        let mut counts = BTreeMap::new();
        for register in &self.registers {
            *counts.entry(register.confidence).or_insert(0usize) += 1;
        }
        format!(
            "Definition confidence: {}",
            counts
                .iter()
                .map(|(level, count)| format!("{} {count}", level.label()))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}
