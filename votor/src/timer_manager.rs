//! Controls the queueing and firing of skip timer events for use
//! in the event loop.

mod stats;
mod timers;

use {
    crate::{common::DELTA_TIMEOUT, event::VotorEvent},
    agave_votor_messages::migration::MigrationStatus,
    crossbeam_channel::Sender,
    parking_lot::RwLock as PlRwLock,
    solana_clock::Slot,
    std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    },
    timers::Timers,
};

/// A manager of timer states.  Uses a background thread to trigger next ready
/// timers and send events.
pub(crate) struct TimerManager {
    timers: Arc<PlRwLock<Timers>>,
    /// Background wall-clock driver thread. `None` in manual/virtual-clock mode
    /// (see [`TimerManager::new_manual`]).
    handle: Option<JoinHandle<()>>,
    /// When set (manual/virtual-clock mode), `set_timeouts` reads the simulator's
    /// virtual clock instead of `Instant::now()`, so scheduled timeouts share the
    /// same clock that [`TimerManager::progress`] is driven with.
    virtual_now: Option<Arc<PlRwLock<Instant>>>,
}

impl TimerManager {
    pub(crate) fn new(
        event_sender: Sender<VotorEvent>,
        exit: Arc<AtomicBool>,
        migration_status: Arc<MigrationStatus>,
    ) -> Self {
        let timers = Arc::new(PlRwLock::new(Timers::new(DELTA_TIMEOUT, event_sender)));
        let handle = {
            let timers = Arc::clone(&timers);
            thread::spawn(move || {
                let _ = migration_status.wait_for_migration_or_exit(exit.as_ref());
                while !exit.load(Ordering::Relaxed) {
                    let duration = match timers.write().progress(Instant::now()) {
                        None => {
                            // No active timers, sleep for an arbitrary amount.
                            // This should be smaller than the minimum amount
                            // of time any newly added timers would take to expire.
                            Duration::from_millis(100)
                        }
                        Some(next_fire) => next_fire.duration_since(Instant::now()),
                    };
                    thread::park_timeout(duration);
                }
            })
        };

        Self {
            timers,
            handle: Some(handle),
            virtual_now: None,
        }
    }

    /// Deterministic, no-thread constructor for the in-process multi-node simulator.
    ///
    /// No background thread is spawned; the caller advances time by writing
    /// `virtual_now` and calling [`TimerManager::progress`]. `set_timeouts`
    /// schedules relative to the same `virtual_now` cell so the two stay on one
    /// clock.
    #[cfg(feature = "dev-context-only-utils")]
    pub(crate) fn new_manual(
        event_sender: Sender<VotorEvent>,
        virtual_now: Arc<PlRwLock<Instant>>,
    ) -> Self {
        let timers = Arc::new(PlRwLock::new(Timers::new(DELTA_TIMEOUT, event_sender)));
        Self {
            timers,
            handle: None,
            virtual_now: Some(virtual_now),
        }
    }

    /// Drive the timer state machines up to `now`, firing any due
    /// `Timeout`/`TimeoutCrashedLeader` events on the event sender. Returns the
    /// next fire instant, if any. Only meaningful in manual/virtual-clock mode.
    #[cfg(feature = "dev-context-only-utils")]
    pub(crate) fn progress(&self, now: Instant) -> Option<Instant> {
        self.timers.write().progress(now)
    }

    pub(crate) fn set_timeouts(
        &self,
        slot: Slot,
        standstill_slot: Option<Slot>,
        delta_first_fec_set: Duration,
        delta_block: Duration,
    ) -> bool {
        let now = match &self.virtual_now {
            Some(virtual_now) => *virtual_now.read(),
            None => Instant::now(),
        };
        let timeout_inserted = self.timers.write().set_timeouts(
            slot,
            now,
            standstill_slot,
            delta_first_fec_set,
            delta_block,
        );
        if timeout_inserted {
            if let Some(handle) = &self.handle {
                handle.thread().unpark();
            }
        }
        timeout_inserted
    }

    pub(crate) fn join(self) {
        if let Some(handle) = self.handle {
            handle.thread().unpark();
            handle.join().unwrap();
        }
    }

    #[cfg(test)]
    pub(crate) fn is_timeout_set(&self, slot: Slot) -> bool {
        self.timers.read().is_timeout_set(slot)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::event::VotorEvent,
        crossbeam_channel::bounded,
        solana_clock::DEFAULT_MS_PER_SLOT,
        std::{assert_matches, time::Duration},
    };

    #[test]
    fn test_timer_manager() {
        let (event_sender, event_receiver) = bounded(1024);
        let exit = Arc::new(AtomicBool::new(false));
        let timer_manager = TimerManager::new(
            event_sender,
            exit.clone(),
            Arc::new(MigrationStatus::post_migration_status()),
        );
        let delta_block = Duration::from_millis(DEFAULT_MS_PER_SLOT);
        let delta_first_fec_set = delta_block;
        let slot = 52;
        let start = Instant::now();
        assert!(timer_manager.set_timeouts(slot, None, delta_first_fec_set, delta_block));
        assert!(!timer_manager.set_timeouts(slot, None, delta_first_fec_set, delta_block));
        // Should see the first two timeout events at DELTA_TIMEOUT + delta_block.
        let mut timeouts_received = 0;
        while timeouts_received < 2 && Instant::now().duration_since(start) < Duration::from_secs(2)
        {
            let recv = event_receiver.recv_timeout(Duration::from_millis(200));
            if let Ok(event) = recv {
                match event {
                    VotorEvent::Timeout(s) => {
                        assert_eq!(s, slot);
                        assert!(
                            Instant::now().duration_since(start) >= DELTA_TIMEOUT + delta_block
                        );
                        timeouts_received += 1;
                    }
                    VotorEvent::TimeoutCrashedLeader(s) => {
                        assert_eq!(s, slot);
                        assert!(
                            Instant::now().duration_since(start)
                                >= DELTA_TIMEOUT + delta_first_fec_set
                        );
                        timeouts_received += 1;
                    }
                    _ => panic!("Unexpected event: {event:?}"),
                }
            }
        }
        assert!(
            timeouts_received == 2,
            "Did not receive all expected timeouts"
        );
        exit.store(true, Ordering::Relaxed);
        timer_manager.join();
    }

    #[cfg(feature = "dev-context-only-utils")]
    #[test]
    fn test_manual_virtual_clock_fires_deterministically() {
        let (event_sender, event_receiver) = bounded(1024);
        let base = Instant::now();
        let virtual_now = Arc::new(PlRwLock::new(base));
        let timer_manager = TimerManager::new_manual(event_sender, virtual_now.clone());

        let delta_block = Duration::from_millis(DEFAULT_MS_PER_SLOT);
        let slot = 4;
        // `set_timeouts` schedules relative to the virtual clock (currently `base`).
        assert!(timer_manager.set_timeouts(slot, None, delta_block, delta_block));

        // Nothing is due yet.
        timer_manager.progress(base);
        assert!(event_receiver.try_recv().unwrap_err().is_empty());

        // Jump the virtual clock far ahead; the whole window's timeouts fire in one progress call.
        let now = base + Duration::from_secs(10);
        *virtual_now.write() = now;
        timer_manager.progress(now);

        let events = event_receiver.try_iter().collect::<Vec<_>>();
        assert!(matches!(
            events.first(),
            Some(VotorEvent::TimeoutCrashedLeader(s)) if *s == slot
        ));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, VotorEvent::Timeout(s) if *s == slot)),
            "expected a Timeout for slot {slot}, got {events:?}"
        );
    }

    #[test]
    fn test_new_earlier_timer_wakes_sleeping_worker() {
        let (event_sender, event_receiver) = bounded(1024);
        let exit = Arc::new(AtomicBool::new(false));
        let timer_manager = TimerManager::new(
            event_sender,
            exit.clone(),
            Arc::new(MigrationStatus::post_migration_status()),
        );

        let old_slot = 52;
        let old_delta = Duration::from_secs(5);
        assert!(timer_manager.set_timeouts(old_slot, None, old_delta, old_delta));
        std::thread::sleep(Duration::from_millis(500));

        let new_slot = 1_000;
        assert!(timer_manager.set_timeouts(
            new_slot,
            None,
            Duration::ZERO,
            Duration::from_millis(1),
        ));

        let event = event_receiver
            .recv_timeout(DELTA_TIMEOUT + Duration::from_millis(500))
            .expect("new earlier timer should wake the sleeping timer worker");
        assert_matches!(event, VotorEvent::TimeoutCrashedLeader(slot) if slot == new_slot);

        exit.store(true, Ordering::Relaxed);
        timer_manager.join();
    }
}
