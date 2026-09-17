use std::collections::HashMap;

use thiserror::Error;

use crate::{
    IDD_CONTROL_MAX_MONITORS, IddAdapterStatus, IddConnectedMonitor, IddDeviceControlError,
    IddDeviceController, IddFrameBrokerMapping, IddFrameSnapshot, IddHandoffError,
    IddMonitorConnectRequest, IddMonitorDisconnectRequest, IddMonitorIdentity, IddMonitorMode,
};

const INVENTORY_STABILITY_ATTEMPTS: usize = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IddCoordinatorInventory {
    pub adapter: IddAdapterStatus,
    pub monitors: Vec<IddConnectedMonitor>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum IddCoordinatorError {
    #[error(transparent)]
    Device(#[from] IddDeviceControlError),
    #[error(transparent)]
    Mapping(#[from] IddHandoffError),
    #[error("the monitor is already prepared with a different mode")]
    ModeConflict,
    #[error("the coordinator already owns its bounded maximum of monitor mappings")]
    CapacityExceeded,
    #[error("the monitor is not prepared by this coordinator")]
    NotPrepared,
    #[error("the driver inventory changed during a bounded consistency check")]
    UnstableInventory,
}

trait MonitorControl {
    fn query(&mut self) -> Result<IddAdapterStatus, IddDeviceControlError>;
    fn enumerate_monitors(&mut self) -> Result<Vec<IddConnectedMonitor>, IddDeviceControlError>;
    fn connect(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<IddConnectedMonitor, IddDeviceControlError>;
    fn disconnect(
        &mut self,
        request: IddMonitorDisconnectRequest,
    ) -> Result<(), IddDeviceControlError>;
}

impl MonitorControl for IddDeviceController {
    fn query(&mut self) -> Result<IddAdapterStatus, IddDeviceControlError> {
        self.query()
    }

    fn enumerate_monitors(&mut self) -> Result<Vec<IddConnectedMonitor>, IddDeviceControlError> {
        self.enumerate_monitors()
    }

    fn connect(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<IddConnectedMonitor, IddDeviceControlError> {
        self.connect(request)
    }

    fn disconnect(
        &mut self,
        request: IddMonitorDisconnectRequest,
    ) -> Result<(), IddDeviceControlError> {
        self.disconnect(request)
    }
}

trait MonitorMappingFactory {
    type Mapping;

    fn create(&mut self, identity: IddMonitorIdentity) -> Result<Self::Mapping, IddHandoffError>;
}

struct WindowsMappingFactory;

impl MonitorMappingFactory for WindowsMappingFactory {
    type Mapping = IddFrameBrokerMapping;

    fn create(&mut self, identity: IddMonitorIdentity) -> Result<Self::Mapping, IddHandoffError> {
        IddFrameBrokerMapping::create_for_monitor(identity)
    }
}

struct PreparedMonitor<M> {
    connected: IddConnectedMonitor,
    mode: IddMonitorMode,
    mapping: M,
}

struct Coordinator<C, F>
where
    F: MonitorMappingFactory,
{
    control: C,
    mapping_factory: F,
    prepared: HashMap<IddMonitorIdentity, PreparedMonitor<F::Mapping>>,
}

impl<C, F> Coordinator<C, F>
where
    C: MonitorControl,
    F: MonitorMappingFactory,
{
    fn new(control: C, mapping_factory: F) -> Self {
        Self {
            control,
            mapping_factory,
            prepared: HashMap::with_capacity(IDD_CONTROL_MAX_MONITORS as usize),
        }
    }

    fn inventory(&mut self) -> Result<IddCoordinatorInventory, IddCoordinatorError> {
        for _ in 0..INVENTORY_STABILITY_ATTEMPTS {
            let before = self.control.enumerate_monitors()?;
            let adapter = self.control.query()?;
            let after = self.control.enumerate_monitors()?;
            if before == after && after.len() == adapter.connected_monitors as usize {
                return Ok(IddCoordinatorInventory {
                    adapter,
                    monitors: after,
                });
            }
        }
        Err(IddCoordinatorError::UnstableInventory)
    }

    fn prepare(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<IddConnectedMonitor, IddCoordinatorError> {
        if let Some(existing) = self.prepared.get(&request.identity) {
            if existing.mode != request.preferred_mode {
                return Err(IddCoordinatorError::ModeConflict);
            }
            return Ok(existing.connected);
        }
        if self.prepared.len() >= IDD_CONTROL_MAX_MONITORS as usize {
            return Err(IddCoordinatorError::CapacityExceeded);
        }

        let inventory = self.inventory()?;
        let stale_identity = inventory
            .monitors
            .iter()
            .any(|monitor| monitor.identity == request.identity);
        if !stale_identity
            && inventory.adapter.connected_monitors >= inventory.adapter.maximum_monitors
        {
            return Err(IddCoordinatorError::CapacityExceeded);
        }
        if stale_identity {
            // A prior Broker may have exited while the driver's producer still
            // owns the old named mapping. Departure closes that driver handle;
            // the new mapping must not be created until this succeeds.
            self.control.disconnect(IddMonitorDisconnectRequest {
                identity: request.identity,
            })?;
        }

        // Mapping ownership precedes monitor arrival so an immediately assigned
        // swapchain can never race ahead of its identity-scoped handoff.
        let mapping = self.mapping_factory.create(request.identity)?;
        let connected = self.control.connect(request)?;
        self.prepared.insert(
            request.identity,
            PreparedMonitor {
                connected,
                mode: request.preferred_mode,
                mapping,
            },
        );
        Ok(connected)
    }

    fn disconnect(&mut self, identity: IddMonitorIdentity) -> Result<bool, IddCoordinatorError> {
        // Keep the mapping live until departure succeeds. On an IOCTL failure
        // local ownership is retained because driver state is unknown.
        self.control
            .disconnect(IddMonitorDisconnectRequest { identity })?;
        Ok(self.prepared.remove(&identity).is_some())
    }

    fn disconnect_all(&mut self) -> Result<(), IddCoordinatorError> {
        let mut monitors: Vec<_> = self
            .prepared
            .values()
            .map(|prepared| prepared.connected)
            .collect();
        monitors.sort_unstable_by_key(|monitor| monitor.connector_index);
        for monitor in monitors {
            self.disconnect(monitor.identity)?;
        }
        Ok(())
    }
}

pub struct IddMonitorCoordinator {
    inner: Coordinator<IddDeviceController, WindowsMappingFactory>,
}

impl IddMonitorCoordinator {
    pub fn open_unique() -> Result<Self, IddCoordinatorError> {
        let controller = IddDeviceController::open_unique()?;
        let mut coordinator = Self {
            inner: Coordinator::new(controller, WindowsMappingFactory),
        };
        coordinator.inventory()?;
        Ok(coordinator)
    }

    pub fn inventory(&mut self) -> Result<IddCoordinatorInventory, IddCoordinatorError> {
        self.inner.inventory()
    }

    pub fn prepare(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<IddConnectedMonitor, IddCoordinatorError> {
        self.inner.prepare(request)
    }

    pub fn disconnect(
        &mut self,
        identity: IddMonitorIdentity,
    ) -> Result<bool, IddCoordinatorError> {
        self.inner.disconnect(identity)
    }

    pub fn disconnect_all(&mut self) -> Result<(), IddCoordinatorError> {
        self.inner.disconnect_all()
    }

    #[must_use]
    pub fn prepared_monitors(&self) -> Vec<IddConnectedMonitor> {
        let mut monitors: Vec<_> = self
            .inner
            .prepared
            .values()
            .map(|prepared| prepared.connected)
            .collect();
        monitors.sort_unstable_by_key(|monitor| monitor.connector_index);
        monitors
    }

    pub fn try_read(
        &mut self,
        identity: IddMonitorIdentity,
    ) -> Result<Option<IddFrameSnapshot>, IddCoordinatorError> {
        self.inner
            .prepared
            .get_mut(&identity)
            .ok_or(IddCoordinatorError::NotPrepared)?
            .mapping
            .try_read()
            .map_err(IddCoordinatorError::Mapping)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashSet, rc::Rc};

    use capyio_core::ids::CapabilityId;

    use super::*;

    #[derive(Clone)]
    struct FakeMappingFactory {
        events: Rc<RefCell<Vec<&'static str>>>,
        live: Rc<RefCell<HashSet<IddMonitorIdentity>>>,
        fail: bool,
    }

    struct FakeMapping {
        identity: IddMonitorIdentity,
        events: Rc<RefCell<Vec<&'static str>>>,
        live: Rc<RefCell<HashSet<IddMonitorIdentity>>>,
    }

    impl Drop for FakeMapping {
        fn drop(&mut self) {
            assert!(self.live.borrow_mut().remove(&self.identity));
            self.events.borrow_mut().push("mapping-drop");
        }
    }

    impl MonitorMappingFactory for FakeMappingFactory {
        type Mapping = FakeMapping;

        fn create(
            &mut self,
            identity: IddMonitorIdentity,
        ) -> Result<Self::Mapping, IddHandoffError> {
            self.events.borrow_mut().push("mapping-create");
            if self.fail {
                return Err(IddHandoffError::AlreadyOwned);
            }
            assert!(self.live.borrow_mut().insert(identity));
            Ok(FakeMapping {
                identity,
                events: Rc::clone(&self.events),
                live: Rc::clone(&self.live),
            })
        }
    }

    struct FakeControl {
        events: Rc<RefCell<Vec<&'static str>>>,
        inventory_results: Vec<Vec<IddConnectedMonitor>>,
        connected_count: u32,
        connect_error: bool,
        disconnect_error: bool,
    }

    type FakeCoordinator = Coordinator<FakeControl, FakeMappingFactory>;
    type FakeFixture = (
        FakeCoordinator,
        Rc<RefCell<Vec<&'static str>>>,
        Rc<RefCell<HashSet<IddMonitorIdentity>>>,
    );

    impl MonitorControl for FakeControl {
        fn query(&mut self) -> Result<IddAdapterStatus, IddDeviceControlError> {
            self.events.borrow_mut().push("query");
            Ok(IddAdapterStatus {
                maximum_monitors: IDD_CONTROL_MAX_MONITORS,
                connected_monitors: self.connected_count,
            })
        }

        fn enumerate_monitors(
            &mut self,
        ) -> Result<Vec<IddConnectedMonitor>, IddDeviceControlError> {
            self.events.borrow_mut().push("enumerate");
            if self.inventory_results.len() > 1 {
                Ok(self.inventory_results.remove(0))
            } else {
                Ok(self.inventory_results.first().cloned().unwrap_or_default())
            }
        }

        fn connect(
            &mut self,
            request: IddMonitorConnectRequest,
        ) -> Result<IddConnectedMonitor, IddDeviceControlError> {
            self.events.borrow_mut().push("connect");
            if self.connect_error {
                return Err(IddDeviceControlError::Ioctl {
                    operation: "connect",
                    code: 5,
                });
            }
            Ok(connected(request.identity, 2))
        }

        fn disconnect(
            &mut self,
            _request: IddMonitorDisconnectRequest,
        ) -> Result<(), IddDeviceControlError> {
            self.events.borrow_mut().push("disconnect");
            if self.disconnect_error {
                return Err(IddDeviceControlError::Ioctl {
                    operation: "disconnect",
                    code: 5,
                });
            }
            Ok(())
        }
    }

    fn identity(index: u8) -> IddMonitorIdentity {
        IddMonitorIdentity::new(
            format!("{index:02x}112233-4455-6677-8899-aabbccddeeff")
                .parse::<CapabilityId>()
                .unwrap(),
        )
        .unwrap()
    }

    fn request(identity: IddMonitorIdentity, width: u32) -> IddMonitorConnectRequest {
        IddMonitorConnectRequest {
            identity,
            preferred_mode: IddMonitorMode {
                width,
                height: 912,
                refresh_millihz: 60_000,
            },
        }
    }

    fn connected(identity: IddMonitorIdentity, connector_index: u32) -> IddConnectedMonitor {
        IddConnectedMonitor {
            identity,
            connector_index,
            target_id: 10 + connector_index,
            adapter_luid_low: 42,
            adapter_luid_high: -3,
        }
    }

    fn coordinator(connect_error: bool, disconnect_error: bool, mapping_fail: bool) -> FakeFixture {
        let events = Rc::new(RefCell::new(Vec::new()));
        let live = Rc::new(RefCell::new(HashSet::new()));
        (
            Coordinator::new(
                FakeControl {
                    events: Rc::clone(&events),
                    inventory_results: vec![vec![]],
                    connected_count: 0,
                    connect_error,
                    disconnect_error,
                },
                FakeMappingFactory {
                    events: Rc::clone(&events),
                    live: Rc::clone(&live),
                    fail: mapping_fail,
                },
            ),
            events,
            live,
        )
    }

    #[test]
    fn prepare_owns_mapping_before_connect_and_is_idempotent() {
        let (mut coordinator, events, live) = coordinator(false, false, false);
        let identity = identity(1);
        let first = coordinator.prepare(request(identity, 1980)).unwrap();
        let second = coordinator.prepare(request(identity, 1980)).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            &*events.borrow(),
            &[
                "enumerate",
                "query",
                "enumerate",
                "mapping-create",
                "connect"
            ]
        );
        assert_eq!(live.borrow().len(), 1);
    }

    #[test]
    fn mapping_or_connect_failure_leaves_no_prepared_monitor() {
        let identity = identity(1);
        let (mut mapping_failure, events, live) = coordinator(false, false, true);
        assert_eq!(
            mapping_failure.prepare(request(identity, 1980)),
            Err(IddCoordinatorError::Mapping(IddHandoffError::AlreadyOwned))
        );
        assert_eq!(
            &*events.borrow(),
            &["enumerate", "query", "enumerate", "mapping-create"]
        );
        assert!(live.borrow().is_empty());

        let (mut connect_failure, events, live) = coordinator(true, false, false);
        assert!(matches!(
            connect_failure.prepare(request(identity, 1980)),
            Err(IddCoordinatorError::Device(IddDeviceControlError::Ioctl {
                operation: "connect",
                code: 5
            }))
        ));
        assert_eq!(
            &*events.borrow(),
            &[
                "enumerate",
                "query",
                "enumerate",
                "mapping-create",
                "connect",
                "mapping-drop"
            ]
        );
        assert!(live.borrow().is_empty());
    }

    #[test]
    fn mode_conflict_does_not_touch_mapping_or_driver() {
        let (mut coordinator, events, _) = coordinator(false, false, false);
        let identity = identity(1);
        coordinator.prepare(request(identity, 1980)).unwrap();
        assert_eq!(
            coordinator.prepare(request(identity, 1280)),
            Err(IddCoordinatorError::ModeConflict)
        );
        assert_eq!(
            &*events.borrow(),
            &[
                "enumerate",
                "query",
                "enumerate",
                "mapping-create",
                "connect"
            ]
        );
    }

    #[test]
    fn disconnect_keeps_mapping_on_unknown_driver_state() {
        let (mut coordinator, events, live) = coordinator(false, true, false);
        let identity = identity(1);
        coordinator.prepare(request(identity, 1980)).unwrap();
        assert!(matches!(
            coordinator.disconnect(identity),
            Err(IddCoordinatorError::Device(IddDeviceControlError::Ioctl {
                operation: "disconnect",
                code: 5
            }))
        ));
        assert_eq!(live.borrow().len(), 1);
        assert_eq!(coordinator.prepared.len(), 1);
        assert_eq!(
            &*events.borrow(),
            &[
                "enumerate",
                "query",
                "enumerate",
                "mapping-create",
                "connect",
                "disconnect"
            ]
        );
    }

    #[test]
    fn successful_disconnect_drops_mapping_after_driver_departure() {
        let (mut coordinator, events, live) = coordinator(false, false, false);
        let identity = identity(1);
        coordinator.prepare(request(identity, 1980)).unwrap();
        assert!(coordinator.disconnect(identity).unwrap());
        assert!(live.borrow().is_empty());
        assert_eq!(
            &*events.borrow(),
            &[
                "enumerate",
                "query",
                "enumerate",
                "mapping-create",
                "connect",
                "disconnect",
                "mapping-drop"
            ]
        );
    }

    #[test]
    fn explicit_disconnect_all_releases_every_prepared_mapping() {
        let (mut coordinator, events, live) = coordinator(false, false, false);
        coordinator.prepare(request(identity(1), 1980)).unwrap();
        coordinator.prepare(request(identity(2), 1980)).unwrap();
        events.borrow_mut().clear();

        coordinator.disconnect_all().unwrap();
        assert!(coordinator.prepared.is_empty());
        assert!(live.borrow().is_empty());
        assert_eq!(
            &*events.borrow(),
            &["disconnect", "mapping-drop", "disconnect", "mapping-drop"]
        );
    }

    #[test]
    fn stale_driver_monitor_departs_before_mapping_recovery() {
        let (mut coordinator, events, live) = coordinator(false, false, false);
        let identity = identity(1);
        let stale = connected(identity, 2);
        coordinator.control.connected_count = 1;
        coordinator.control.inventory_results = vec![vec![stale]];

        assert_eq!(coordinator.prepare(request(identity, 1980)).unwrap(), stale);
        assert_eq!(live.borrow().len(), 1);
        assert_eq!(
            &*events.borrow(),
            &[
                "enumerate",
                "query",
                "enumerate",
                "disconnect",
                "mapping-create",
                "connect"
            ]
        );
    }

    #[test]
    fn full_foreign_inventory_fails_before_mapping_or_connect() {
        let (mut coordinator, events, live) = coordinator(false, false, false);
        coordinator.control.connected_count = IDD_CONTROL_MAX_MONITORS;
        coordinator.control.inventory_results = vec![
            (0..IDD_CONTROL_MAX_MONITORS)
                .map(|index| connected(identity(index as u8), index))
                .collect(),
        ];
        assert_eq!(
            coordinator.prepare(request(identity(20), 1980)),
            Err(IddCoordinatorError::CapacityExceeded)
        );
        assert!(live.borrow().is_empty());
        assert_eq!(&*events.borrow(), &["enumerate", "query", "enumerate"]);
    }

    #[test]
    fn inventory_requires_two_equal_snapshots_and_matching_count() {
        let (mut coordinator, events, _) = coordinator(false, false, false);
        let monitor = connected(identity(1), 2);
        coordinator.control.connected_count = 1;
        coordinator.control.inventory_results = vec![vec![], vec![monitor], vec![monitor]];
        assert_eq!(
            coordinator.inventory().unwrap(),
            IddCoordinatorInventory {
                adapter: IddAdapterStatus {
                    maximum_monitors: 8,
                    connected_monitors: 1,
                },
                monitors: vec![monitor],
            }
        );
        assert_eq!(
            &*events.borrow(),
            &[
                "enumerate",
                "query",
                "enumerate",
                "enumerate",
                "query",
                "enumerate"
            ]
        );
    }
}
