//! Explicitly linked per-core breakpoints. Equal locations alone never imply a group.
use super::*;
use crate::{config::BreakpointOptions, session::Breakpoint};

impl Coordinator {
    pub(super) fn annotate_breakpoints(&self, snapshot: &mut Snapshot) {
        for b in &mut snapshot.breakpoints {
            b.cores = if let Some(group) = &b.group {
                self.engines
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        e.snapshot
                            .breakpoints
                            .iter()
                            .any(|v| v.group.as_ref() == Some(group))
                            .then_some(i)
                    })
                    .collect()
            } else {
                vec![self.active]
            };
        }
    }

    pub(super) fn begin_breakpoint_edit(&mut self, req: &Request) -> bool {
        if !matches!(
            req.method.as_str(),
            "break_cores" | "update_break" | "enable_break" | "delete_break"
        ) {
            return false;
        }
        let number = req
            .params
            .get("number")
            .and_then(Json::as_str)
            .unwrap_or("");
        let selected: Vec<_> = self.engines[self.active]
            .snapshot
            .breakpoints
            .iter()
            .filter(|b| b.id == number || (number.is_empty() && req.method != "break_cores"))
            .cloned()
            .collect();
        if req.method != "break_cores" && !selected.iter().any(|b| b.group.is_some()) {
            return false;
        }
        match self.breakpoint_steps(req, &selected) {
            Ok(steps) => {
                self.control_batch(req.clone(), steps, false);
                self.batch.as_mut().unwrap().break_undo = Some(VecDeque::new());
            }
            Err(error) => self.reply(req.id, Json::Null, Some(error)),
        }
        true
    }

    fn breakpoint_steps(
        &self,
        req: &Request,
        selected: &[Breakpoint],
    ) -> Result<VecDeque<Step>, String> {
        if selected.is_empty() {
            return Err("Select an existing breakpoint".into());
        }
        let desired = if req.method == "break_cores" {
            if selected.len() != 1 {
                return Err("Select one breakpoint".into());
            }
            let list = req
                .params
                .get("cores")
                .and_then(Json::as_array)
                .ok_or("cores must be an array of indices")?;
            let indices = list
                .iter()
                .map(|v| {
                    v.as_u64()
                        .and_then(|i| usize::try_from(i).ok())
                        .filter(|i| *i < self.engines.len())
                        .ok_or("Unknown breakpoint core")
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !indices.contains(&self.active) {
                return Err(
                    "Keep the selected core checked; switch cores before moving ownership".into(),
                );
            }
            let mut unique = indices.clone();
            unique.sort_unstable();
            unique.dedup();
            if unique.len() != indices.len() {
                return Err("Duplicate breakpoint core".into());
            }
            Some(unique)
        } else {
            None
        };
        if req.method == "update_break"
            && req
                .params
                .get("number")
                .and_then(Json::as_str)
                .is_none_or(str::is_empty)
        {
            return Err("Select one breakpoint to edit".into());
        }
        if req.method == "enable_break"
            && req
                .params
                .get("number")
                .and_then(Json::as_str)
                .is_none_or(str::is_empty)
            && req.params.get("all").and_then(Json::as_bool) != Some(true)
        {
            return Err("Enable all requires all=true".into());
        }
        let mut steps = VecDeque::new();
        let mut groups = std::collections::HashSet::new();
        for b in selected {
            if let Some(g) = &b.group
                && !groups.insert(g.clone())
            {
                continue;
            }
            if desired.is_some()
                && (b.options().kind.is_data() || b.temporary || b.id.contains('.'))
            {
                return Err("Core selection supports persistent code/hardware breakpoints".into());
            }
            let group = b.group.clone().unwrap_or_else(|| {
                format!(
                    "bp-{}-{}-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos(),
                    self.active,
                    b.id
                )
            });
            let mut members: Vec<(usize, Breakpoint)> = self
                .engines
                .iter()
                .enumerate()
                .flat_map(|(i, e)| {
                    e.snapshot
                        .breakpoints
                        .iter()
                        .filter(move |v| {
                            if let Some(group) = &b.group {
                                v.group.as_ref() == Some(group)
                            } else {
                                i == self.active && v.id == b.id
                            }
                        })
                        .cloned()
                        .map(move |v| (i, v))
                })
                .collect();
            members.sort_by_key(|(i, _)| *i);
            if members.windows(2).any(|w| w[0].0 == w[1].0) {
                return Err(
                    "Duplicate group records on a core; repair the configuration first".into(),
                );
            }
            let wanted: Vec<_> = desired
                .clone()
                .unwrap_or_else(|| members.iter().map(|(i, _)| *i).collect());
            let mut affected = wanted.clone();
            affected.extend(members.iter().map(|(i, _)| *i));
            affected.sort_unstable();
            affected.dedup();
            // Check every affected core before sending the first mutation.
            for &i in &affected {
                let e = &self.engines[i];
                if e.exited
                    || e.unresponsive
                    || !matches!(e.snapshot.state.as_str(), "READY" | "STOPPED")
                {
                    return Err(format!(
                        "[{}] Pause/connect every affected core before editing this breakpoint",
                        e.name
                    ));
                }
            }
            // Insert/update wanted members first; only then remove excluded peers.
            affected.sort_by_key(|i| !wanted.contains(i));
            for i in affected {
                let existing = members.iter().find(|(core, _)| *core == i).map(|(_, b)| b);
                let before = existing.map(Breakpoint::options);
                let after = if req.method == "delete_break" || !wanted.contains(&i) {
                    None
                } else {
                    let mut o = existing.unwrap_or(b).options();
                    if desired.is_some() {
                        o.group = (wanted.len() > 1).then(|| group.clone());
                    }
                    if matches!(req.method.as_str(), "enable_break" | "update_break") {
                        apply_options(&mut o, req)?;
                    }
                    Some(o)
                };
                if before == after {
                    continue;
                }
                let number = existing.map(|v| v.id.as_str());
                let params = json!({"group":group,"number":number,"options":after});
                let undo = json!({"group":group,"number":number,"options":before});
                steps.push_back(Step::Breakpoint(i, params, Some(undo)));
            }
        }
        Ok(steps)
    }
}

fn apply_options(o: &mut BreakpointOptions, req: &Request) -> Result<(), String> {
    if req.method == "enable_break" || req.params.get("enabled").is_some() {
        o.enabled = req
            .params
            .get("enabled")
            .and_then(Json::as_bool)
            .ok_or("enabled must be true or false")?;
    }
    if req.method == "update_break" {
        if let Some(v) = req.params.get("condition") {
            o.condition = v.as_str().ok_or("condition must be text")?.into();
        }
        if let Some(v) = req.params.get("ignore_count") {
            o.ignore_count = v
                .as_u64()
                .filter(|v| *v <= i32::MAX as u64)
                .ok_or("Ignore count must be 0..2147483647")? as u32;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Coordinator, Receiver<Event>) {
        let p = Project {
            cores: (0..3)
                .map(|i| Core {
                    name: format!("cpu{i}"),
                    endpoint: format!("local{i}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let (tx, rx) = mpsc::sync_channel(512);
        let mut c = Coordinator::new(p, tx, Arc::new(AtomicBool::new(false)));
        c.active = 0;
        for e in &mut c.engines {
            e.snapshot.state = "STOPPED".into();
        }
        c.engines[0].snapshot.breakpoints.push(Breakpoint {
            id: "4".into(),
            location: "tick".into(),
            enabled: true,
            kind: "breakpoint".into(),
            ..Default::default()
        });
        (c, rx)
    }
    #[test]
    fn source_break_stays_local_and_invalid_members_never_start_edits() {
        let (mut c, rx) = fixture();
        c.begin(Request::new(1, "break", json!({"location":"main"})));
        assert!(
            matches!(c.batch.as_ref().unwrap().steps.front(), Some(Step::Core(0, m)) if m == "break")
        );
        c.batch = None;
        for cores in [json!([]), json!([1]), json!([0, 0]), json!([0, 9])] {
            c.begin(Request::new(
                2,
                "break_cores",
                json!({"number":"4","cores":cores}),
            ));
            assert!(c.batch.is_none());
        }
        c.engines[1].snapshot.state = "RUNNING".into();
        c.begin(Request::new(
            3,
            "break_cores",
            json!({"number":"4","cores":[0,1]}),
        ));
        assert!(c.batch.is_none());
        assert!(
            rx.try_iter()
                .filter(|e| matches!(e, Event::Response { ok: false, .. }))
                .count()
                >= 5
        );
    }
    #[test]
    fn equal_locations_are_independent_and_group_routes_real_per_core_ids() {
        let (mut c, _rx) = fixture();
        let mut other = c.engines[0].snapshot.breakpoints[0].clone();
        other.id = "19".into();
        c.engines[1].snapshot.breakpoints.push(other);
        c.begin(Request::new(
            1,
            "break_cores",
            json!({"number":"4","cores":[0,1]}),
        ));
        let steps = &c.batch.as_ref().unwrap().steps;
        assert_eq!(steps.len(), 2);
        assert!(matches!(&steps[1], Step::Breakpoint(1,p,_) if p["number"].is_null()));
        c.batch = None;
        c.engines[0].snapshot.breakpoints[0].group = Some("linked".into());
        let mut linked = c.engines[0].snapshot.breakpoints[0].clone();
        linked.id = "23".into();
        c.engines[1].snapshot.breakpoints.push(linked);
        c.begin(Request::new(2, "delete_break", json!({"number":"4"})));
        let steps = &c.batch.as_ref().unwrap().steps;
        assert!(
            matches!(&steps[1], Step::Breakpoint(1,p,_) if p["number"] == "23" && p["options"].is_null())
        );
        let snap = c.snapshot();
        assert_eq!(snap.breakpoints[0].cores, [0, 1]);
        c.active = 1;
        assert_eq!(c.snapshot().breakpoints[0].cores, [1]);
    }
    #[test]
    fn failed_member_replays_all_undo_steps_and_retains_rollback_errors() {
        let (mut c, _rx) = fixture();
        c.begin(Request::new(
            1,
            "break_cores",
            json!({"number":"4","cores":[0,1]}),
        ));
        let mut b = c.batch.take().unwrap();
        b.break_undo
            .as_mut()
            .unwrap()
            .push_back(Step::Breakpoint(0, json!({"undo":true}), None));
        c.fail(&mut b, "[cpu1] no hardware resources".into());
        assert!(b.recovering && b.steps.len() == 1);
        c.fail(&mut b, "[cpu0] disconnected".into());
        assert_eq!(b.steps.len(), 1);
        assert!(b.errors[1].contains("Rollback"));
    }
}
