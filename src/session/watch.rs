//! Lazy Watch trees built with native MI variable objects, without Python.
//! Each refresh creates and deletes its own objects in the selected frame/core.
use super::*;
use std::collections::{BTreeMap, HashMap};

pub(super) type Expansions = HashMap<String, BTreeMap<Vec<usize>, usize>>;
const PAGE: usize = 32;
const MAX_NODES: usize = 256;
const MAX_DEPTH: usize = 8;

impl Engine {
    pub(super) fn refresh_watches(&mut self) {
        self.snapshot.watches = self
            .watch_names
            .clone()
            .into_iter()
            .take(64)
            .map(|name| self.read_watch(&name))
            .collect();
    }

    fn read_watch(&mut self, expression: &str) -> Variable {
        let old = self
            .snapshot
            .watches
            .iter()
            .find(|v| v.name == expression)
            .cloned();
        let result = self.mi(&format!("-var-create - * {}", mi::quote(expression)));
        let mut value = match result {
            Ok(record) => {
                let object = record.data.string("name");
                let expansions = self
                    .watch_expansions
                    .get(expression)
                    .cloned()
                    .unwrap_or_default();
                let mut remaining = MAX_NODES;
                let value = self.read_watch_node(
                    expression,
                    &record.data,
                    vec![],
                    &expansions,
                    &mut remaining,
                );
                // Root deletion also frees every child, including failed expansions.
                let _ = self.mi(&format!("-var-delete {}", mi::quote(&object)));
                value
            }
            Err(_) => {
                // Preserve scalar/expression support on GDBs without variable objects.
                let result = self.mi(&format!(
                    "-data-evaluate-expression {}",
                    mi::quote(expression)
                ));
                let (value, error) = match result {
                    Ok(record) => (record.data.string("value"), false),
                    Err(error) => (error, true),
                };
                Variable {
                    name: expression.into(),
                    value,
                    error,
                    ..Default::default()
                }
            }
        };
        mark_changes(&mut value, old.as_ref());
        value
    }

    fn read_watch_node(
        &mut self,
        name: &str,
        data: &Value,
        path: Vec<usize>,
        expansions: &BTreeMap<Vec<usize>, usize>,
        remaining: &mut usize,
    ) -> Variable {
        *remaining = remaining.saturating_sub(1);
        let child_count = data.string("numchild").parse().unwrap_or(0);
        let mut tree = WatchTree {
            path,
            type_name: data.string("type"),
            child_count,
            ..Default::default()
        };
        let mut value = data.string("value");
        let mut error = false;
        if value.is_empty() && child_count == 0 && !tree.type_name.is_empty() {
            // GDB may create an unreadable scalar successfully with value="".
            // Ask the object for its diagnostic instead of showing a valid-looking type.
            match self.mi(&format!(
                "-var-evaluate-expression {}",
                mi::quote(&data.string("name"))
            )) {
                Ok(record) => {
                    value = record.data.string("value");
                    if value.is_empty() {
                        value = "<unavailable>".into();
                    }
                }
                Err(message) => {
                    value = message;
                    error = true;
                }
            }
        }
        if value.is_empty() {
            value = format!("<{}>", tree.type_name);
        }
        error |= value.contains("<error reading variable")
            || matches!(value.as_str(), "<unavailable>" | "<optimized out>");
        if let Some(&count) = expansions.get(&tree.path).filter(|_| child_count > 0) {
            tree.expanded = true;
            let count = count.min(child_count).min(*remaining);
            if tree.path.len() < MAX_DEPTH && count > 0 {
                match self.mi(&format!(
                    "-var-list-children --all-values {} 0 {count}",
                    mi::quote(&data.string("name"))
                )) {
                    Ok(record) => {
                        if let Some(children) = record.data.field("children") {
                            for (index, child) in children.items().iter().take(count).enumerate() {
                                if *remaining == 0 {
                                    break;
                                }
                                let child = child.field("child").unwrap_or(child);
                                let mut path = tree.path.clone();
                                path.push(index);
                                tree.children.push(self.read_watch_node(
                                    &child.string("exp"),
                                    child,
                                    path,
                                    expansions,
                                    remaining,
                                ));
                            }
                        }
                    }
                    Err(message) => {
                        value = message;
                        error = true;
                    }
                }
            }
            tree.has_more = tree.children.len() < child_count;
            tree.limited = tree.has_more && (*remaining == 0 || tree.path.len() >= MAX_DEPTH);
        }
        Variable {
            name: name.into(),
            value,
            error,
            tree: Some(tree),
            ..Default::default()
        }
    }

    pub(super) fn expand_watch(&mut self, params: &Json) -> Result<Json, String> {
        let expression = params["expression"]
            .as_str()
            .ok_or("Watch expression is required")?;
        let path: Vec<usize> = match params.get("path") {
            Some(value) => {
                serde_json::from_value(value.clone()).map_err(|_| "Invalid Watch child path")?
            }
            None => vec![],
        };
        if path.len() > MAX_DEPTH {
            return Err("Watch expansion depth limit reached".into());
        }
        let root = self
            .snapshot
            .watches
            .iter()
            .position(|v| v.name == expression)
            .ok_or("Watch no longer exists")?;
        let tree = find_node(&mut self.snapshot.watches[root], &path)
            .and_then(|v| v.tree.as_mut())
            .ok_or("Watch node no longer exists")?;
        if tree.child_count == 0 {
            return Err("This Watch value has no children".into());
        }
        let expanded = params["expanded"].as_bool().unwrap_or(!tree.expanded);
        if !expanded {
            tree.expanded = false;
            // Collapsing and deleting never access the target, even while running.
            if let Some(paths) = self.watch_expansions.get_mut(expression) {
                paths.remove(&path);
            }
        } else {
            if tree.limited && tree.expanded {
                return Err("Watch expansion limit: 256 nodes per expression, 8 levels".into());
            }
            self.stopped()?;
            let paths = self.watch_expansions.entry(expression.into()).or_default();
            let count = paths.entry(path).or_insert(PAGE);
            if params["more"].as_bool().unwrap_or(false) {
                *count = count.saturating_add(PAGE).min(MAX_NODES);
            }
            let value = self.read_watch(expression);
            self.snapshot.watches[root] = value;
        }
        self.publish();
        Ok(json!({"expanded":expanded}))
    }
}

fn find_node<'a>(root: &'a mut Variable, path: &[usize]) -> Option<&'a mut Variable> {
    let mut node = root;
    for &index in path {
        node = node.tree.as_mut()?.children.get_mut(index)?;
    }
    Some(node)
}

fn mark_changes(value: &mut Variable, old: Option<&Variable>) {
    value.changed = old.is_some_and(|old| old.name == value.name && old.value != value.value);
    if let Some(tree) = &mut value.tree {
        let old_tree = old
            .and_then(|v| v.tree.as_ref())
            .filter(|t| t.type_name == tree.type_name);
        for (index, child) in tree.children.iter_mut().enumerate() {
            mark_changes(child, old_tree.and_then(|t| t.children.get(index)));
        }
    }
}
