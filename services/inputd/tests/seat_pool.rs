use korri_inputd::{
    input_seat::{GamepadState, SeatBackend, SeatSpec},
    seat_pool::{PoolError, SeatPool, DEFAULT_SEAT_COUNT},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU8,
    sync::{Arc, Mutex},
};

const SESSION: &str = "0123456789abcdef0123456789abcdef";
const NEXT_SESSION: &str = "fedcba9876543210fedcba9876543210";
const HELD: GamepadState = GamepadState {
    buttons: 0x1000,
    left_trigger: 255,
    right_trigger: 128,
    left_stick_x: i16::MIN,
    left_stick_y: i16::MAX,
    right_stick_x: -100,
    right_stick_y: 100,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    Create(SeatSpec),
    Write(u8, GamepadState),
    Destroy(u8),
    Drop,
}

#[derive(Default)]
struct Record {
    calls: Vec<Call>,
    live: BTreeMap<u8, GamepadState>,
    fail_create: Option<u8>,
    fail_write: BTreeSet<u8>,
    fail_destroy: BTreeSet<u8>,
}

#[derive(Clone, Default)]
struct Probe(Arc<Mutex<Record>>);

impl Probe {
    fn calls(&self) -> Vec<Call> {
        self.0.lock().unwrap().calls.clone()
    }

    fn state(&self, slot: u8) -> GamepadState {
        self.0.lock().unwrap().live[&slot]
    }

    fn fail_write(&self, slot: u8, fail: bool) {
        let mut record = self.0.lock().unwrap();
        if fail {
            record.fail_write.insert(slot);
        } else {
            record.fail_write.remove(&slot);
        }
    }

    fn assert_released(&self, slots: u8) {
        let record = self.0.lock().unwrap();
        assert!(record.live.is_empty());
        assert_eq!(
            record
                .calls
                .iter()
                .filter_map(|call| match call {
                    Call::Destroy(slot) => Some(*slot),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            (1..=slots).rev().collect::<Vec<_>>()
        );
        assert_eq!(record.calls.last(), Some(&Call::Drop));
        assert_eq!(
            record
                .calls
                .iter()
                .filter(|call| **call == Call::Drop)
                .count(),
            1
        );
    }
}

struct Backend(Probe);

impl SeatBackend for Backend {
    fn create(&mut self, spec: &SeatSpec) -> Result<(), String> {
        let mut record = self.0 .0.lock().unwrap();
        record.calls.push(Call::Create(spec.clone()));
        // Deliberately start non-neutral: construction must establish neutral.
        record.live.insert(spec.slot, HELD);
        if record.fail_create == Some(spec.slot) {
            return Err(format!("create {} failed", spec.slot));
        }
        Ok(())
    }

    fn write_state(&mut self, slot: u8, state: GamepadState) -> Result<(), String> {
        let mut record = self.0 .0.lock().unwrap();
        record.calls.push(Call::Write(slot, state));
        if record.fail_write.contains(&slot) {
            // Simulate a partial effect before failure, not an atomic no-op.
            record.live.get_mut(&slot).unwrap().buttons = HELD.buttons;
            return Err(format!("write {slot} failed"));
        }
        *record.live.get_mut(&slot).unwrap() = state;
        Ok(())
    }

    fn destroy(&mut self, slot: u8) -> Result<(), String> {
        let mut record = self.0 .0.lock().unwrap();
        record.calls.push(Call::Destroy(slot));
        if record.fail_destroy.contains(&slot) {
            return Err(format!("destroy {slot} failed"));
        }
        record.live.remove(&slot);
        Ok(())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        let mut record = self.0 .0.lock().unwrap();
        record.calls.push(Call::Drop);
        record.live.clear();
    }
}

type Pool = SeatPool<u8, Backend>;

fn pool(count: NonZeroU8) -> (Pool, Probe) {
    let probe = Probe::default();
    let pool = Pool::new(count, Backend(probe.clone())).unwrap();
    (pool, probe)
}

fn one_seat() -> (Pool, Probe) {
    pool(NonZeroU8::new(1).unwrap())
}

#[test]
fn default_four_are_created_neutral_and_allocated_in_arrival_order() {
    assert_eq!(DEFAULT_SEAT_COUNT.get(), 4);
    let (mut pool, probe) = pool(DEFAULT_SEAT_COUNT);
    assert_eq!(
        probe.calls(),
        (1..=4)
            .flat_map(|slot| [
                Call::Create(SeatSpec::for_slot(slot)),
                Call::Write(slot, GamepadState::neutral()),
            ])
            .collect::<Vec<_>>()
    );
    for (source, slot) in [(7, 1), (4, 2), (12, 3), (0, 4)] {
        assert_eq!(pool.connect(source), Ok(slot));
        assert_eq!(probe.state(slot), GamepadState::neutral());
    }
    assert_eq!(pool.connect(9), Err(PoolError::NoSeat));
    let calls = probe.calls();
    assert_eq!(pool.connect(4), Ok(2));
    assert_eq!(probe.calls(), calls);
}

#[test]
fn explicit_count_above_four_keeps_identities_across_source_and_session_churn() {
    // Engine-only evidence, not proof of installed udev permissions or hardware.
    let (mut pool, probe) = pool(NonZeroU8::new(6).unwrap());
    for session in [SESSION, NEXT_SESSION] {
        pool.begin_session(session).unwrap();
        for source in 0..6 {
            assert_eq!(pool.connect(source), Ok(source + 1));
            pool.write_state(&source, HELD).unwrap();
            pool.disconnect(&source).unwrap();
        }
        assert_eq!(pool.connect(9), Err(PoolError::NoSeat));
        assert_eq!(pool.connect(4), Ok(5));
        pool.disconnect(&4).unwrap();
        pool.end_session(session).unwrap();
        for slot in 1..=6 {
            assert_eq!(probe.state(slot), GamepadState::neutral());
        }
    }
    assert_eq!(
        probe
            .calls()
            .into_iter()
            .filter(|call| matches!(call, Call::Create(_) | Call::Destroy(_)))
            .collect::<Vec<_>>(),
        (1..=6)
            .map(|slot| Call::Create(SeatSpec::for_slot(slot)))
            .collect::<Vec<_>>()
    );
    pool.stop().unwrap();
    probe.assert_released(6);
}

#[test]
fn opaque_source_keys_share_one_pool_without_clone_or_serialization() {
    // These keys represent caller namespaces, NOT normalized physical identities.
    #[derive(Eq, PartialEq)]
    enum Key {
        ProducerA(u8),
        ProducerB(u8),
    }
    let probe = Probe::default();
    let mut pool = SeatPool::new(DEFAULT_SEAT_COUNT, Backend(probe.clone())).unwrap();
    assert_eq!(pool.connect(Key::ProducerA(7)), Ok(1));
    assert_eq!(pool.connect(Key::ProducerB(7)), Ok(2));
    assert_eq!(pool.connect(Key::ProducerA(7)), Ok(1));
    pool.write_state(&Key::ProducerA(7), HELD).unwrap();
    assert_eq!(probe.state(1), HELD);
    assert_eq!(probe.state(2), GamepadState::neutral());
    pool.write_state(&Key::ProducerB(7), HELD).unwrap();
    pool.disconnect(&Key::ProducerA(7)).unwrap();
    assert_eq!(probe.state(1), GamepadState::neutral());
    assert_eq!(probe.state(2), HELD);
    assert_eq!(pool.connect(Key::ProducerB(8)), Ok(1));
}

#[test]
fn disconnect_outside_session_neutralizes_and_frees_first_slot_without_destroying() {
    let (mut pool, probe) = pool(DEFAULT_SEAT_COUNT);
    for source in 0..4 {
        pool.connect(source).unwrap();
        pool.write_state(&source, HELD).unwrap();
    }
    pool.disconnect(&3).unwrap();
    pool.disconnect(&1).unwrap();
    assert_eq!(probe.state(2), GamepadState::neutral());
    assert_eq!(probe.state(4), GamepadState::neutral());
    assert_eq!(pool.connect(10), Ok(2));
    assert_eq!(pool.connect(11), Ok(4));
    assert_eq!(pool.connect(1), Err(PoolError::NoSeat));
    assert!(!probe
        .calls()
        .iter()
        .any(|call| matches!(call, Call::Destroy(_))));
}

#[test]
fn reservation_lasts_through_pause_until_exact_session_end() {
    let (mut pool, probe) = one_seat();
    pool.begin_session(SESSION).unwrap();
    pool.connect(7).unwrap();
    pool.write_state(&7, HELD).unwrap();
    pool.disconnect(&7).unwrap();
    assert_eq!(probe.state(1), GamepadState::neutral());
    let calls = probe.calls();
    // Pause has no transition in this engine. The same session remains bound,
    // irrespective of elapsed time or repeated connection/disconnection attempts.
    for _ in 0..10 {
        assert_eq!(pool.connect(8), Err(PoolError::NoSeat));
        assert_eq!(pool.disconnect(&7), Ok(1));
        assert_eq!(
            pool.write_state(&7, HELD),
            Err(PoolError::DisconnectedSource)
        );
    }
    assert_eq!(probe.calls(), calls);
    assert_eq!(pool.connect(7), Ok(1));
    pool.write_state(&7, HELD).unwrap();
    pool.disconnect(&7).unwrap();
    pool.end_session(SESSION).unwrap();
    assert_eq!(pool.connect(8), Ok(1));
}

#[test]
fn session_end_releases_only_disconnected_and_retains_connected_slots() {
    let (mut pool, probe) = pool(DEFAULT_SEAT_COUNT);
    for source in 0..4 {
        pool.connect(source).unwrap();
    }
    // Sources can connect before a session begins.
    pool.begin_session(SESSION).unwrap();
    pool.disconnect(&1).unwrap();
    pool.disconnect(&3).unwrap();
    pool.end_session(SESSION).unwrap();
    assert_eq!(pool.connect(2), Ok(3));
    assert_eq!(pool.connect(0), Ok(1));
    assert_eq!(pool.connect(10), Ok(2));
    assert_eq!(pool.connect(11), Ok(4));
    // End removed session scope: a subsequent disconnect frees immediately.
    pool.write_state(&2, HELD).unwrap();
    pool.disconnect(&2).unwrap();
    assert_eq!(probe.state(3), GamepadState::neutral());
    assert_eq!(pool.connect(12), Ok(3));
}

#[test]
fn session_validation_and_exact_identity_reject_stale_end_and_replacement() {
    let (mut pool, probe) = one_seat();
    for invalid in ["", "session", "0123456789ABCDEF0123456789ABCDEF"] {
        assert!(matches!(
            pool.begin_session(invalid),
            Err(PoolError::InvalidSession(_))
        ));
    }
    assert_eq!(pool.end_session(SESSION), Err(PoolError::StaleSession));
    pool.begin_session(SESSION).unwrap();
    pool.connect(7).unwrap();
    pool.disconnect(&7).unwrap();
    let calls = probe.calls();
    assert_eq!(
        pool.begin_session(SESSION),
        Err(PoolError::SessionAlreadyActive)
    );
    assert_eq!(
        pool.begin_session(NEXT_SESSION),
        Err(PoolError::SessionAlreadyActive)
    );
    assert_eq!(pool.end_session(NEXT_SESSION), Err(PoolError::StaleSession));
    assert_eq!(pool.connect(8), Err(PoolError::NoSeat));
    assert_eq!(probe.calls(), calls);
    pool.end_session(SESSION).unwrap();
    pool.begin_session(NEXT_SESSION).unwrap();
    pool.connect(8).unwrap();
    pool.disconnect(&8).unwrap();
    assert_eq!(pool.end_session(SESSION), Err(PoolError::StaleSession));
    assert_eq!(pool.connect(9), Err(PoolError::NoSeat));
    pool.end_session(NEXT_SESSION).unwrap();
    assert_eq!(pool.connect(9), Ok(1));
}

#[test]
fn unknown_and_disconnected_sources_cannot_write_or_allocate_implicitly() {
    let (mut pool, probe) = one_seat();
    let calls = probe.calls();
    assert_eq!(pool.write_state(&7, HELD), Err(PoolError::UnknownSource));
    assert_eq!(pool.disconnect(&7), Err(PoolError::UnknownSource));
    assert_eq!(probe.calls(), calls);
    pool.begin_session(SESSION).unwrap();
    pool.connect(7).unwrap();
    pool.disconnect(&7).unwrap();
    assert_eq!(
        pool.write_state(&7, HELD),
        Err(PoolError::DisconnectedSource)
    );
    assert_eq!(probe.calls(), calls);
}

#[test]
fn duplicate_connect_and_state_do_not_clear_or_rewrite_held_state() {
    let (mut pool, probe) = one_seat();
    pool.connect(7).unwrap();
    pool.write_state(&7, HELD).unwrap();
    let calls = probe.calls();
    for _ in 0..10 {
        assert_eq!(pool.connect(7), Ok(1));
        assert_eq!(pool.write_state(&7, HELD), Ok(1));
    }
    assert_eq!(probe.calls(), calls);
    assert_eq!(probe.state(1), HELD);
}

#[test]
fn failed_state_write_is_unknown_and_retry_is_not_deduplicated() {
    let (mut pool, probe) = one_seat();
    pool.connect(7).unwrap();
    probe.fail_write(1, true);
    assert!(matches!(
        pool.write_state(&7, HELD),
        Err(PoolError::Backend(_))
    ));
    assert_eq!(pool.connect(8), Err(PoolError::NoSeat));
    probe.fail_write(1, false);
    // Last successful state was neutral, but the failed write partially held it.
    let calls = probe.calls().len();
    pool.write_state(&7, GamepadState::neutral()).unwrap();
    assert_eq!(probe.calls().len(), calls + 1);
    assert_eq!(probe.state(1), GamepadState::neutral());
    probe.fail_write(1, true);
    assert!(pool.write_state(&7, HELD).is_err());
    probe.fail_write(1, false);
    pool.write_state(&7, HELD).unwrap();
    assert_eq!(probe.state(1), HELD);
}

#[test]
fn failed_disconnect_blocks_reuse_and_writes_until_retry_neutralizes() {
    let (mut pool, probe) = one_seat();
    pool.connect(7).unwrap();
    pool.write_state(&7, HELD).unwrap();
    probe.fail_write(1, true);
    assert!(matches!(pool.disconnect(&7), Err(PoolError::Backend(_))));
    assert_eq!(pool.connect(8), Err(PoolError::NoSeat));
    assert_eq!(
        pool.write_state(&7, HELD),
        Err(PoolError::DisconnectedSource)
    );
    assert!(matches!(pool.connect(7), Err(PoolError::Backend(_))));
    assert_eq!(pool.connect(8), Err(PoolError::NoSeat));
    probe.fail_write(1, false);
    assert_eq!(pool.disconnect(&7), Ok(1));
    assert_eq!(probe.state(1), GamepadState::neutral());
    assert_eq!(pool.connect(8), Ok(1));
}

#[test]
fn same_source_can_retry_reconnect_after_failed_neutralization() {
    let (mut pool, probe) = one_seat();
    pool.begin_session(SESSION).unwrap();
    pool.connect(7).unwrap();
    pool.write_state(&7, HELD).unwrap();
    probe.fail_write(1, true);
    assert!(pool.disconnect(&7).is_err());
    assert!(pool.connect(7).is_err());
    probe.fail_write(1, false);
    assert_eq!(pool.connect(7), Ok(1));
    assert_eq!(probe.state(1), GamepadState::neutral());
    pool.write_state(&7, HELD).unwrap();
    pool.end_session(SESSION).unwrap();
    assert_eq!(pool.connect(8), Err(PoolError::NoSeat));
    assert_eq!(pool.connect(7), Ok(1));
}

#[test]
fn failed_session_end_retains_failed_reservation_and_exact_id_for_retry() {
    let (mut pool, probe) = pool(NonZeroU8::new(3).unwrap());
    pool.begin_session(SESSION).unwrap();
    for source in 0..3 {
        pool.connect(source).unwrap();
        pool.write_state(&source, HELD).unwrap();
    }
    probe.fail_write(1, true);
    probe.fail_write(2, true);
    assert!(pool.disconnect(&0).is_err());
    assert!(pool.disconnect(&1).is_err());
    probe.fail_write(2, false);
    assert!(matches!(
        pool.end_session(SESSION),
        Err(PoolError::Backend(_))
    ));
    // End still visits later slots after the first failure. Only successful
    // releases become available. Connected slot three remains assigned.
    assert_eq!(probe.state(2), GamepadState::neutral());
    assert_eq!(pool.connect(8), Ok(2));
    assert_eq!(pool.connect(9), Err(PoolError::NoSeat));
    assert_eq!(pool.connect(2), Ok(3));
    assert_eq!(
        pool.begin_session(NEXT_SESSION),
        Err(PoolError::SessionAlreadyActive)
    );
    assert_eq!(pool.end_session(NEXT_SESSION), Err(PoolError::StaleSession));
    probe.fail_write(1, false);
    pool.end_session(SESSION).unwrap();
    assert_eq!(probe.state(1), GamepadState::neutral());
    assert_eq!(pool.connect(9), Ok(1));
    pool.begin_session(NEXT_SESSION).unwrap();
}

#[test]
fn construction_failure_rolls_back_created_devices_and_drops_backend() {
    let probe = Probe::default();
    probe.0.lock().unwrap().fail_create = Some(3);
    assert!(matches!(
        Pool::new(DEFAULT_SEAT_COUNT, Backend(probe.clone())),
        Err(PoolError::Backend(_))
    ));
    probe.assert_released(2);
    assert_eq!(
        probe
            .calls()
            .iter()
            .filter(|call| matches!(call, Call::Create(_)))
            .count(),
        3
    );
}

#[test]
fn construction_neutralization_failure_reports_rollback_failure_and_releases_all() {
    let probe = Probe::default();
    probe.fail_write(2, true);
    probe.0.lock().unwrap().fail_destroy.insert(2);
    let error = match Pool::new(DEFAULT_SEAT_COUNT, Backend(probe.clone())) {
        Err(PoolError::Backend(error)) => error,
        _ => panic!("expected backend error"),
    };
    assert!(error.contains("write 2 failed"));
    assert!(error.contains("rollback:"));
    assert!(error.contains("destroy 2 failed"));
    probe.assert_released(2);
}

#[test]
fn stop_neutralizes_every_held_seat_before_destroy_and_does_not_double_cleanup() {
    let (mut pool, probe) = pool(DEFAULT_SEAT_COUNT);
    for source in 0..4 {
        pool.connect(source).unwrap();
        pool.write_state(&source, HELD).unwrap();
    }
    let start = probe.calls().len();
    pool.stop().unwrap();
    let mut expected: Vec<_> = (1..=4)
        .map(|slot| Call::Write(slot, GamepadState::neutral()))
        .collect();
    expected.extend((1..=4).rev().map(Call::Destroy));
    expected.push(Call::Drop);
    assert_eq!(&probe.calls()[start..], expected.as_slice());
    probe.assert_released(4);
}

#[test]
fn stop_failures_do_not_skip_other_neutralizations_or_destroys() {
    let (mut pool, probe) = pool(DEFAULT_SEAT_COUNT);
    for source in 0..4 {
        pool.connect(source).unwrap();
        pool.write_state(&source, HELD).unwrap();
    }
    probe.fail_write(1, true);
    probe.0.lock().unwrap().fail_destroy.insert(4);
    let start = probe.calls().len();
    let error = pool.stop().unwrap_err();
    assert!(matches!(error, PoolError::Backend(message)
        if message.contains("write 1 failed") && message.contains("destroy 4 failed")));
    for slot in 1..=4 {
        assert!(probe.calls()[start..].contains(&Call::Write(slot, GamepadState::neutral())));
    }
    probe.assert_released(4);
}

#[test]
fn idle_resize_retains_surviving_devices_and_connected_assignments() {
    let (mut pool, probe) = pool(DEFAULT_SEAT_COUNT);
    pool.connect(7).unwrap();
    pool.write_state(&7, HELD).unwrap();
    let start = probe.calls().len();
    pool.resize(NonZeroU8::new(6).unwrap()).unwrap();
    assert_eq!(pool.count(), 6);
    assert_eq!(pool.slot(&7), Some(1));
    assert_eq!(probe.state(1), GamepadState::neutral());
    assert!(!probe.calls()[start..]
        .iter()
        .any(|call| matches!(call, Call::Destroy(_))));
    for source in 8..=12 {
        pool.connect(source).unwrap();
    }
    pool.resize(NonZeroU8::new(2).unwrap()).unwrap();
    assert_eq!(pool.count(), 2);
    assert_eq!(pool.slot(&7), Some(1));
    assert_eq!(pool.slot(&8), Some(2));
    assert_eq!(pool.slot(&9), None);
    assert_eq!(probe.0.lock().unwrap().live.len(), 2);
}

#[test]
fn resize_refuses_even_identical_count_during_session_and_reports_partial_failure() {
    let (mut pool, probe) = pool(DEFAULT_SEAT_COUNT);
    pool.begin_session(SESSION).unwrap();
    assert_eq!(
        pool.resize(DEFAULT_SEAT_COUNT),
        Err(PoolError::SessionAlreadyActive)
    );
    pool.end_session(SESSION).unwrap();
    probe.0.lock().unwrap().fail_create = Some(6);
    assert!(matches!(
        pool.resize(NonZeroU8::new(6).unwrap()),
        Err(PoolError::Backend(_))
    ));
    // This backend deliberately retains resources even from failed create.
    // The receiver treats the failure as fatal; backend drop releases them.
    drop(pool);
    assert!(probe.0.lock().unwrap().live.is_empty());
}

#[test]
fn neutral_route_barrier_does_not_end_session_or_drop_assignments() {
    let (mut pool, probe) = one_seat();
    pool.begin_session(SESSION).unwrap();
    pool.connect(7).unwrap();
    pool.write_state(&7, HELD).unwrap();
    pool.neutralize().unwrap();
    assert_eq!(probe.state(1), GamepadState::neutral());
    assert_eq!(pool.session(), Some(SESSION));
    assert_eq!(pool.slot(&7), Some(1));
}

#[test]
fn drop_cleans_up_even_during_session_with_failed_source_loss() {
    let (mut pool, probe) = one_seat();
    pool.begin_session(SESSION).unwrap();
    pool.connect(7).unwrap();
    pool.write_state(&7, HELD).unwrap();
    probe.fail_write(1, true);
    assert!(pool.disconnect(&7).is_err());
    probe.fail_write(1, false);
    let start = probe.calls().len();
    drop(pool);
    assert_eq!(
        &probe.calls()[start..],
        [
            Call::Write(1, GamepadState::neutral()),
            Call::Destroy(1),
            Call::Drop
        ]
    );
    probe.assert_released(1);
}
