//! Coordinate access to each OpenOCD service across core workers and TCL clients.
//! A lease covers a request or a complete read/write/verify transaction. Nested
//! MI/TCL requests on the owning worker retain it until the final lease is dropped.
use crate::config::Project;
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
    net::{SocketAddr, ToSocketAddrs},
    rc::Rc,
    sync::{Arc, Condvar, Mutex, OnceLock},
    thread::{self, ThreadId},
};

#[derive(Default)]
struct State {
    fault: Option<String>,
    owner: Option<ThreadId>,
    depth: usize,
}

pub(crate) struct Service {
    address: SocketAddr,
    state: Mutex<State>,
    available: Condvar,
}

pub(crate) struct Lease<'a> {
    service: &'a Service,
    // Ownership belongs to the worker thread, including nested MI/TCL calls.
    // A lease must never move to another thread.
    _not_send: PhantomData<Rc<()>>,
}

static SERVICES: OnceLock<Mutex<BTreeMap<SocketAddr, Arc<Service>>>> = OnceLock::new();

pub(crate) fn service(address: SocketAddr) -> Result<Arc<Service>, String> {
    let address = canonical(address);
    let mut services = SERVICES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "OpenOCD service registry is poisoned; restart to recover")?;
    if !services.contains_key(&address) && services.len() >= 4096 {
        return Err("Too many OpenOCD services in this process".into());
    }
    Ok(services
        .entry(address)
        .or_insert_with(|| {
            Arc::new(Service {
                address,
                state: Mutex::default(),
                available: Condvar::new(),
            })
        })
        .clone())
}

fn canonical(address: SocketAddr) -> SocketAddr {
    match address {
        SocketAddr::V6(address) if address.ip().to_ipv4_mapped().is_some() => SocketAddr::new(
            address.ip().to_ipv4_mapped().unwrap().into(),
            address.port(),
        ),
        address => address,
    }
}

fn addresses(endpoint: &str) -> Result<Vec<SocketAddr>, String> {
    endpoint
        .to_socket_addrs()
        .map(|addresses| addresses.take(16).collect())
        .map_err(|error| format!("Invalid TCL endpoint {endpoint}: {error}"))
}

pub(crate) fn check_endpoint(endpoint: &str) -> Result<(), String> {
    for address in addresses(endpoint)? {
        let service = service(address)?;
        // Do not hold a lease during TCP connection setup. transact acquires it
        // again before sending, which also closes the check/connect race.
        service.acquire(false)?;
    }
    Ok(())
}

pub(crate) fn for_project(project: &Project) -> Result<Vec<Arc<Service>>, String> {
    let mut endpoints = vec![project.registers.tcl_endpoint.as_str()];
    endpoints.extend(
        project
            .memory_access
            .iter()
            .map(|channel| channel.tcl_endpoint.as_str()),
    );
    endpoints.extend(
        project
            .live_watch
            .iter()
            .map(|watch| watch.tcl_endpoint.as_str()),
    );
    endpoints.extend(project.sync.iter().map(|sync| sync.tcl_endpoint.as_str()));
    let addresses: BTreeSet<_> = endpoints
        .into_iter()
        .filter(|endpoint| !endpoint.is_empty())
        // An unavailable optional channel does not disable an unrelated GDB
        // session. It reports its own resolution error when actually requested.
        .filter_map(|endpoint| addresses(endpoint).ok())
        .flatten()
        .map(canonical)
        .collect();
    // Deterministic order prevents deadlocks when a project uses several services.
    addresses.into_iter().map(service).collect()
}

pub(crate) fn recover(project: &Project) -> Result<(), String> {
    for service in for_project(project)? {
        service.acquire(true)?.clear_fault();
    }
    Ok(())
}

impl Service {
    pub(crate) fn acquire(&self, allow_fault: bool) -> Result<Lease<'_>, String> {
        let mut state = self.state.lock().map_err(|_| {
            format!(
                "OpenOCD service {} is poisoned; restart to recover",
                self.address
            )
        })?;
        let owner = thread::current().id();
        loop {
            if !allow_fault && let Some(error) = &state.fault {
                return Err(format!(
                    "OpenOCD service {} stopped after uncertain access; reconnect to recover: {error}",
                    self.address
                ));
            }
            if state.owner.is_none() || state.owner == Some(owner) {
                state.depth = state
                    .depth
                    .checked_add(1)
                    .ok_or("Too many nested debug transactions")?;
                state.owner = Some(owner);
                return Ok(Lease {
                    service: self,
                    _not_send: PhantomData,
                });
            }
            state = self.available.wait(state).map_err(|_| {
                format!(
                    "OpenOCD service {} lock is poisoned; restart to recover",
                    self.address
                )
            })?;
        }
    }
}

impl Lease<'_> {
    pub(crate) fn quarantine(&mut self, error: &str) {
        if let Ok(mut state) = self.service.state.lock() {
            state.fault.get_or_insert_with(|| error.into());
        }
    }
    fn clear_fault(&mut self) {
        if let Ok(mut state) = self.service.state.lock() {
            state.fault = None;
        }
    }
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.service.state.lock() {
            debug_assert_eq!(state.owner, Some(thread::current().id()));
            state.depth = state.depth.saturating_sub(1);
            if state.depth == 0 {
                state.owner = None;
                self.service.available.notify_all();
            }
        } else {
            self.service.available.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    fn unique() -> (SocketAddr, std::net::TcpListener) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        (listener.local_addr().unwrap(), listener)
    }

    #[test]
    fn ipv4_mapped_aliases_do_not_acquire_the_same_service_twice() {
        let mut project = Project::default();
        project.registers.tcl_endpoint = "127.0.0.1:6637".into();
        project.memory_access.push(crate::config::MemoryAccess {
            tcl_endpoint: "[::ffff:127.0.0.1]:6637".into(),
            ..Default::default()
        });
        assert_eq!(for_project(&project).unwrap().len(), 1);
    }

    #[test]
    fn a_service_serializes_core_and_bus_requests_but_independent_services_do_not_block() {
        let (address, _listener) = unique();
        let (other, _other_listener) = unique();
        let service = service(address).unwrap();
        let lease = service.acquire(false).unwrap();
        let independent = super::service(other).unwrap();
        let _other_lease = independent.acquire(false).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let service = super::service(address).unwrap();
            entered_tx.send(()).unwrap();
            let _lease = service.acquire(false).unwrap();
            done_tx.send(()).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
        drop(lease);
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn faults_are_shared_with_other_clients_and_only_explicit_recovery_clears_them() {
        let (address, _listener) = unique();
        let first = service(address).unwrap();
        first
            .acquire(false)
            .unwrap()
            .quarantine("target restoration failed");
        let second = service(address).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(second.acquire(false).is_err());
        assert!(check_endpoint(&address.to_string()).is_err());
        let mut project = Project::default();
        project.registers.tcl_endpoint = address.to_string();
        recover(&project).unwrap();
        assert!(second.acquire(false).is_ok());
    }

    #[test]
    fn complete_transactions_allow_nested_io_but_hold_other_workers_until_the_last_lease() {
        let (address, _listener) = unique();
        let service = service(address).unwrap();
        let outer = service.acquire(false).unwrap();
        let inner = service.acquire(false).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            started_tx.send(()).unwrap();
            let service = super::service(address).unwrap();
            let _lease = service.acquire(false).unwrap();
            done_tx.send(()).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(outer);
        assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
        drop(inner);
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn faults_from_nested_io_block_the_rest_of_the_transaction_and_waiting_workers() {
        let (address, _listener) = unique();
        let service = service(address).unwrap();
        let _outer = service.acquire(false).unwrap();
        service
            .acquire(false)
            .unwrap()
            .quarantine("write response lost");
        assert!(service.acquire(false).is_err());
        let _cleanup = service.acquire(true).unwrap();
        drop(_cleanup);
        drop(_outer);
        assert!(service.acquire(false).is_err());
    }
}
