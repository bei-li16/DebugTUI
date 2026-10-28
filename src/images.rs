//! Resident symbol images. Inspecting files and resolving a PC never controls a target.
use crate::config::{Project, SourceMap};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Image {
    pub id: String,
    pub elf: PathBuf,
    pub cores: Vec<String>,
    pub source_root: PathBuf,
    pub source_map: Vec<SourceMap>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub memory_channel: String,
    pub offset: i64,
    pub sections: BTreeMap<String, u64>,
    /// Optional stopped-program mode constraint; never a Debug permission.
    pub domain: Domain,
    /// Explicit opt-in to the diagnostic ABI; never infer a saved SP from x[13].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_context: Option<SavedContext>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SavedContext {
    pub provider: String,
    pub symbol: String,
    pub index: u32,
    pub cpu: u32,
    pub vm: u32,
    pub vcpu: u32,
    pub channel: String,
    pub memory: Vec<MemoryRange>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRange {
    pub start: u64,
    pub size: u64,
}

/// Explicit objfile qualification, independent of C++ namespaces and of drive
/// letters inside the expression (for example @vm0:D:/src/main.c:42).
pub fn scoped_expression(text: &str) -> Result<(Option<&str>, &str), String> {
    let Some(rest) = text.strip_prefix('@') else {
        return Ok((None, text));
    };
    let (id, expression) = rest
        .split_once(':')
        .ok_or("Use @image:location or @image:expression")?;
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || expression.trim().is_empty()
    {
        return Err("Use a valid image id and nonempty expression after '@image:'".into());
    }
    Ok((Some(id), expression))
}

/// A qualified source reference is transport metadata, not an OS path.
pub fn qualified_source(image: Option<&str>, file: &str) -> String {
    if file.is_empty() {
        return String::new();
    }
    let raw = scoped_expression(file).map(|(_, raw)| raw).unwrap_or(file);
    image
        .map(|id| format!("@{id}:{raw}"))
        .unwrap_or_else(|| file.into())
}

pub(crate) fn source_maps(project: &Project, image: &Image) -> Vec<SourceMap> {
    if !project.source_mapping_enabled() {
        return vec![];
    }
    let mut seen = BTreeSet::new();
    let mut maps = image
        .source_map
        .iter()
        .cloned()
        .chain(project.effective_source_maps())
        .filter(|map| {
            seen.insert(
                crate::source::normalized(&map.from)
                    .trim_end_matches('/')
                    .to_owned(),
            )
        })
        .collect::<Vec<_>>();
    maps.sort_by_key(|m| std::cmp::Reverse(m.from.len()));
    maps
}

impl MemoryRange {
    pub fn contains(&self, address: u64, bytes: u64) -> bool {
        bytes > 0
            && self.start <= address
            && self
                .start
                .checked_add(self.size)
                .zip(address.checked_add(bytes))
                .is_some_and(|(end, requested)| requested <= end)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    #[default]
    Any,
    Hyp,
    Guest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutableRange {
    pub section: String,
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Loaded {
    pub id: String,
    pub elf: PathBuf,
    pub primary: bool,
    pub domain: Domain,
    pub bits: u8,
    pub machine: u16,
    pub has_debug_info: bool,
    /// Content identity for cache invalidation, not a firmware authenticity proof.
    pub content_revision: String,
    pub executable_ranges: Vec<ExecutableRange>,
    pub allocated_ranges: Vec<AllocatedRange>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AllocatedRange {
    pub section: String,
    pub linked_start: u64,
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resolution {
    pub state: String,
    pub image: Option<String>,
    pub candidates: Vec<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub symbols_loaded: bool,
    pub images: Vec<Loaded>,
    pub execution: Resolution,
    pub selected_frame: Resolution,
    pub symbol_epoch: u64,
    pub program_cpsr: Option<u32>,
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_guest: Option<serde_json::Value>,
}
impl State {
    pub fn cache_key(&self) -> String {
        format!(
            "{}:{:?}:{}:{}",
            self.symbol_epoch,
            self.selected_frame.image,
            self.error,
            self.images
                .iter()
                .map(|i| format!("{}={}", i.id, i.content_revision))
                .collect::<Vec<_>>()
                .join(";")
        )
    }
}

pub fn validate(project: &Project) -> Result<(), String> {
    if project.images.len() > 64 {
        return Err("At most 64 symbol images are supported".into());
    }
    let mut ids = BTreeSet::new();
    for image in &project.images {
        if image.id.is_empty()
            || image.id.len() > 64
            || !image
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err("Image id must contain 1..64 letters, digits, '.', '_' or '-'".into());
        }
        if !ids.insert(&image.id) {
            return Err(format!("Duplicate image id: {}", image.id));
        }
        if image.elf.as_os_str().is_empty() {
            return Err(format!("Image {} requires an ELF", image.id));
        }
        if image.id == "main" && image.elf != project.program.elf {
            return Err("Image id main is reserved for program.elf".into());
        }
        if image
            .sections
            .keys()
            .any(|s| s.is_empty() || s.chars().any(char::is_control))
        {
            return Err("Image section names must be nonempty single-line strings".into());
        }
        for core in &image.cores {
            if core != "default" && !project.cores.iter().any(|c| &c.name == core) {
                return Err(format!("Image {} refers to unknown core {core}", image.id));
            }
        }
        if let Some(saved) = &image.saved_context {
            if saved.provider != "bao_v1" || image.domain != Domain::Guest {
                return Err("Saved context requires a Guest image and provider bao_v1".into());
            }
            if saved.index > 255
                || saved.symbol.is_empty()
                || !saved
                    .symbol
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || saved.symbol.as_bytes()[0].is_ascii_digit()
                || saved.channel.is_empty()
                || saved.memory.is_empty()
                || saved.memory.len() > 32
                || saved.memory.iter().any(|r| {
                    r.size == 0
                        || r.start
                            .checked_add(r.size)
                            .is_none_or(|e| e > 0x1_0000_0000)
                })
            {
                return Err("Saved context needs a C symbol, index 0..255, memory channel and bounded ARM32 RAM/code ranges".into());
            }
        }
        if image
            .source_map
            .iter()
            .any(|m| m.from.is_empty() || m.from.chars().any(char::is_control))
        {
            return Err("Image source map prefixes must be nonempty single-line strings".into());
        }
    }
    Ok(())
}

pub fn applicable(project: &Project) -> Vec<&Image> {
    let core = project.preference_core.as_deref().unwrap_or("default");
    project
        .images
        .iter()
        .filter(|i| i.cores.is_empty() || i.cores.iter().any(|c| c == core))
        .collect()
}

pub fn inspect_project(project: &Project) -> Result<Vec<Loaded>, String> {
    if project.images.is_empty() {
        return Ok(vec![]);
    }
    let mut images = vec![];
    if !project.program.elf.as_os_str().is_empty() {
        let primary = Image {
            id: "main".into(),
            elf: project.program.elf.clone(),
            ..Default::default()
        };
        if applicable(project)
            .iter()
            .filter(|i| i.elf == primary.elf)
            .count()
            > 1
        {
            return Err("Only one image may describe the primary ELF on a core".into());
        }
        let defined = applicable(project)
            .into_iter()
            .find(|i| i.elf == primary.elf);
        if defined.is_some_and(|i| i.offset != 0 || !i.sections.is_empty()) {
            return Err("Primary ELF must retain its linked addresses; use an additional image for relocated symbols".into());
        }
        images.push(inspect(defined.unwrap_or(&primary), true)?);
    }
    for image in applicable(project) {
        if image.elf == project.program.elf {
            continue;
        }
        let loaded = inspect(image, false)?;
        if images
            .first()
            .is_some_and(|main| main.machine != loaded.machine || main.bits != loaded.bits)
        {
            return Err(format!(
                "Image {} architecture differs from the primary ELF",
                image.id
            ));
        }
        images.push(loaded);
    }
    Ok(images)
}

pub fn inspect(image: &Image, primary: bool) -> Result<Loaded, String> {
    let metadata = fs::metadata(&image.elf).map_err(|e| format!("Image {}: {e}", image.id))?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return Err("ELF must be a file no larger than 128 MiB".into());
    }
    let data = fs::read(&image.elf).map_err(|e| e.to_string())?;
    let mut loaded = parse(image, &data)?;
    loaded.primary = primary;
    Ok(loaded)
}

fn parse(image: &Image, data: &[u8]) -> Result<Loaded, String> {
    let bad = || format!("Invalid or unsupported ELF for image {}", image.id);
    if data.get(..4) != Some(b"\x7fELF") || data.get(6) != Some(&1) {
        return Err(bad());
    }
    let bits = match data.get(4) {
        Some(1) => 32,
        Some(2) => 64,
        _ => return Err(bad()),
    };
    let little = match data.get(5) {
        Some(1) => true,
        Some(2) => false,
        _ => return Err(bad()),
    };
    let read = |at: usize, n: usize| -> Result<u64, String> {
        let bytes = data
            .get(at..at.checked_add(n).ok_or_else(bad)?)
            .ok_or_else(bad)?;
        Ok(if little {
            bytes.iter().rev().fold(0, |v, b| (v << 8) | u64::from(*b))
        } else {
            bytes.iter().fold(0, |v, b| (v << 8) | u64::from(*b))
        })
    };
    if ![2, 3].contains(&read(16, 2)?) {
        return Err("Symbol images must be linked executable/shared ELF files".into());
    }
    let machine = read(18, 2)? as u16;
    let (shoff, stride, count, strings) = if bits == 32 {
        (read(32, 4)?, read(46, 2)?, read(48, 2)?, read(50, 2)?)
    } else {
        (read(40, 8)?, read(58, 2)?, read(60, 2)?, read(62, 2)?)
    };
    let shoff = usize::try_from(shoff).map_err(|_| bad())?;
    let stride = usize::try_from(stride).map_err(|_| bad())?;
    let count = usize::try_from(count).map_err(|_| bad())?;
    if count == 0
        || strings as usize >= count
        || stride < if bits == 32 { 40 } else { 64 }
        || shoff
            .checked_add(stride.checked_mul(count).ok_or_else(bad)?)
            .is_none_or(|end| end > data.len())
    {
        return Err(bad());
    }
    let string_header = shoff + strings as usize * stride;
    let (offset_at, size_at, width) = if bits == 32 { (16, 20, 4) } else { (24, 32, 8) };
    let string_offset =
        usize::try_from(read(string_header + offset_at, width)?).map_err(|_| bad())?;
    let string_size = usize::try_from(read(string_header + size_at, width)?).map_err(|_| bad())?;
    let names = data
        .get(string_offset..string_offset.checked_add(string_size).ok_or_else(bad)?)
        .ok_or_else(bad)?;
    let mut ranges = vec![];
    let mut allocated = vec![];
    let mut found_sections = BTreeSet::new();
    let mut debug = false;
    for index in 0..count {
        let at = shoff + index * stride;
        let name_offset = read(at, 4)? as usize;
        let tail = names.get(name_offset..).ok_or_else(bad)?;
        let end = tail.iter().position(|b| *b == 0).ok_or_else(bad)?;
        let name = std::str::from_utf8(&tail[..end]).map_err(|_| bad())?;
        debug |= matches!(name, ".debug_info" | ".zdebug_info");
        let flags = read(at + 8, if bits == 32 { 4 } else { 8 })?;
        let address = read(
            at + if bits == 32 { 12 } else { 16 },
            if bits == 32 { 4 } else { 8 },
        )?;
        let size = read(at + size_at, width)?;
        found_sections.insert(name.to_owned());
        if flags & 2 == 0 || size == 0 {
            continue;
        }
        let start = if let Some(address) = image.sections.get(name) {
            *address
        } else {
            u64::try_from(i128::from(address) + i128::from(image.offset))
                .map_err(|_| "ELF relocation address overflow".to_owned())?
        };
        if flags & 6 == 6 && size != 0 {
            ranges.push(ExecutableRange {
                section: name.into(),
                start,
                end: start
                    .checked_add(size)
                    .ok_or("ELF executable range overflow")?,
            });
        }
        if flags & 2 != 0 && size != 0 {
            allocated.push(AllocatedRange {
                section: name.into(),
                linked_start: address,
                start,
                end: start
                    .checked_add(size)
                    .ok_or("ELF allocated range overflow")?,
            });
        }
    }
    if image.sections.keys().any(|s| !found_sections.contains(s)) {
        return Err("An image section override does not exist in the ELF".into());
    }
    if ranges.is_empty() {
        return Err("ELF has no allocated executable sections".into());
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hash);
    Ok(Loaded {
        id: image.id.clone(),
        elf: image.elf.clone(),
        primary: false,
        domain: image.domain,
        bits,
        machine,
        has_debug_info: debug,
        content_revision: format!("{:016x}:{}", hash.finish(), data.len()),
        executable_ranges: ranges,
        allocated_ranges: allocated,
    })
}

pub fn resolve(images: &[Loaded], pc: u64, mode: Option<u32>) -> Resolution {
    let pc = pc & !1;
    let candidates: Vec<_> = images
        .iter()
        .filter(|i| {
            i.executable_ranges
                .iter()
                .any(|r| r.start <= pc && pc < r.end)
        })
        .filter(|i| {
            mode.is_none_or(|m| match i.domain {
                Domain::Any => true,
                Domain::Hyp => m & 31 == 0x1a,
                Domain::Guest => matches!(m & 31, 0x10 | 0x11 | 0x12 | 0x13 | 0x17 | 0x1b | 0x1f),
            })
        })
        .map(|i| i.id.clone())
        .collect();
    let (state, image, detail) = match candidates.as_slice() {
        [id] => (
            "resolved",
            Some(id.clone()),
            "PC belongs to one configured executable image; this is not Debug permission",
        ),
        [] => (
            "unknown",
            None,
            "PC/mode does not identify a configured executable image",
        ),
        _ => (
            "ambiguous",
            None,
            "Overlapping executable images require an independently established address-space identity",
        ),
    };
    Resolution {
        state: state.into(),
        image,
        candidates,
        detail: detail.into(),
    }
}

pub fn changed(image: &Loaded, definition: &Image) -> Result<bool, String> {
    Ok(inspect(definition, image.primary)?.content_revision != image.content_revision)
}

pub fn source_path(image: &Image, file: &str) -> Option<PathBuf> {
    let file = crate::source::normalized(file);
    let mut maps = image.source_map.iter().collect::<Vec<_>>();
    maps.sort_by_key(|m| std::cmp::Reverse(m.from.len()));
    for map in maps {
        if let Some(tail) = crate::source::suffix(&file, &crate::source::normalized(&map.from)) {
            let path = map.to.join(tail);
            return path.is_file().then_some(path);
        }
    }
    let path = image.source_root.join(file);
    path.is_file().then_some(path)
}

pub(crate) fn absolute_paths(images: &mut [Image], base: &Path) {
    for image in images {
        if !image.elf.is_absolute() {
            image.elf = base.join(&image.elf);
        }
        image.source_root = base.join(&image.source_root);
        for map in &mut image.source_map {
            if !map.to.is_absolute() {
                map.to = base.join(&map.to);
            }
        }
    }
}
