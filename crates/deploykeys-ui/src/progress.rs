//! Live state for the single progress line under the header.
//!
//! The line is the aggregate of every in-flight operation, real or simulated:
//!
//! - Real operations: reported by the Tauri backend as `progress` events, each
//!   carrying an operation id (`auth.sign_in`, `repos.sync`, …) and a percent in
//!   [0, 100].
//! - Simulated operations: short local waits (open URL, switch language, sign
//!   out) with no backend stream. They climb on a timer toward a cap, then jump
//!   to full once the caller marks them done.
//!
//! Completion is deliberately not instant. When the last operation finishes the
//! bar fills to 100 %, holds for a beat, then fades out. Removing it the moment
//! work ended made it disappear mid-bar; the fill-hold-fade reads as "done".

use leptos::*;
use std::collections::HashMap;
use std::time::Duration;

/// Simulated climb: step every `SIM_INTERVAL`, never past `SIM_CAP` so there is
/// visible headroom for the jump to 100 % on completion.
const SIM_STEP: u8 = 6;
const SIM_CAP: u8 = 90;
const SIM_INTERVAL: Duration = Duration::from_millis(90);

/// Completion choreography. `HOLD` must outlast the width transition so the bar
/// visibly reaches 100 % before fading; `FADE` must outlast the opacity
/// transition so the fade finishes before the bar leaves the DOM. Both
/// transitions live in `.header-progress-bar` (styles/input.css).
const HOLD: Duration = Duration::from_millis(240);
const FADE: Duration = Duration::from_millis(340);

/// Lifecycle of the shared bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Nothing in flight: bar absent from the DOM.
    Hidden,
    /// At least one operation in flight: width tracks the aggregate.
    Running,
    /// All work done: width pinned to 100 %, still fully opaque.
    Hold,
    /// Pinned at 100 % while opacity animates to 0.
    Fading,
}

/// Shared handle to the global progress bar state.
#[derive(Clone, Copy, Debug)]
pub struct ProgressHandle {
    entries: RwSignal<HashMap<String, ProgressEntry>>,
    phase: RwSignal<Phase>,
    /// Bumped on every transition; lets a new operation cancel the pending
    /// hold/fade timers of a completion that is still in flight.
    generation: RwSignal<u64>,
    /// Counter for generating stable ids for simulated operations.
    next_sim_id: RwSignal<u64>,
}

#[derive(Clone, Copy, Debug)]
struct ProgressEntry {
    percent: u8,
    is_real: bool,
}

impl ProgressHandle {
    pub fn new() -> Self {
        Self {
            entries: RwSignal::new(HashMap::new()),
            phase: RwSignal::new(Phase::Hidden),
            generation: RwSignal::new(0),
            next_sim_id: RwSignal::new(0),
        }
    }

    pub fn provide(self) {
        provide_context(self);
    }

    pub fn expect() -> Self {
        use_context::<Self>().expect("ProgressHandle provided at app root")
    }

    /// Receive a real progress checkpoint from the backend.
    pub fn on_real_progress(&self, operation: String, percent: u8) {
        if percent >= 100 {
            self.entries.update(|map| {
                map.remove(&operation);
            });
            self.maybe_complete();
            return;
        }
        self.entries.update(|map| {
            map.insert(
                operation,
                ProgressEntry {
                    percent,
                    is_real: true,
                },
            );
        });
        self.start_running();
    }

    /// Start a simulated short operation and return its id so the caller can
    /// mark it done. While active, the bar advances on a timer.
    pub fn begin_simulated(&self) -> String {
        let id = self.next_sim_id.get_untracked();
        self.next_sim_id.update(|n| *n += 1);
        let key = format!("sim.{id}");
        self.entries.update(|map| {
            map.insert(
                key.clone(),
                ProgressEntry {
                    percent: 0,
                    is_real: false,
                },
            );
        });
        self.start_running();
        self.advance_simulated(key.clone());
        key
    }

    /// Mark a simulated operation complete. Removing its entry both stops the
    /// climb timer (see `advance_simulated`) and lets the bar settle to 100 %.
    pub fn end_simulated(&self, key: &str) {
        self.entries.update(|map| {
            map.remove(key);
        });
        self.maybe_complete();
    }

    /// A new operation appeared: show the bar and cancel any pending fade.
    fn start_running(&self) {
        self.generation.update(|g| *g += 1);
        self.phase.set(Phase::Running);
    }

    /// Called after an operation is removed. If it was the last one, run the
    /// fill -> hold -> fade -> reset choreography. The generation snapshot turns
    /// the timers into no-ops if a new operation starts before they fire.
    fn maybe_complete(&self) {
        if !self.entries.get_untracked().is_empty() {
            return;
        }
        self.generation.update(|g| *g += 1);
        let token = self.generation.get_untracked();
        let phase = self.phase;
        let generation = self.generation;

        phase.set(Phase::Hold);

        set_timeout(
            move || {
                if generation.get_untracked() == token {
                    phase.set(Phase::Fading);
                }
            },
            HOLD,
        );
        set_timeout(
            move || {
                if generation.get_untracked() == token {
                    phase.set(Phase::Hidden);
                }
            },
            HOLD + FADE,
        );
    }

    fn advance_simulated(&self, key: String) {
        let handle = *self;
        set_timeout(
            move || {
                let mut alive = false;
                handle.entries.update(|map| {
                    if let Some(entry) = map.get_mut(&key) {
                        if !entry.is_real {
                            entry.percent = (entry.percent + SIM_STEP).min(SIM_CAP);
                            alive = true;
                        }
                    }
                });
                // Stop once the operation is gone. Without this the timer keeps
                // running forever and clamps a just-finished bar from 100 % back
                // down to the cap, making it lurch backwards before vanishing.
                if alive {
                    handle.advance_simulated(key);
                }
            },
            SIM_INTERVAL,
        );
    }

    fn aggregate(map: &HashMap<String, ProgressEntry>) -> u8 {
        if map.is_empty() {
            return 0;
        }
        let total: u32 = map.values().map(|e| e.percent as u32).sum();
        (total / map.len() as u32).min(100) as u8
    }

    /// Bar width in percent for the current phase. Climbs with the aggregate
    /// while running, then pins to full for the hold + fade.
    pub fn bar_width(&self) -> Signal<u8> {
        let entries = self.entries;
        let phase = self.phase;
        Signal::derive(move || match phase.get() {
            Phase::Hidden => 0,
            Phase::Running => Self::aggregate(&entries.get()),
            Phase::Hold | Phase::Fading => 100,
        })
    }

    /// Whether the bar is fully opaque. Drops during `Fading` so the fade only
    /// begins after the bar has reached 100 %.
    pub fn bar_opaque(&self) -> Signal<bool> {
        let phase = self.phase;
        Signal::derive(move || matches!(phase.get(), Phase::Running | Phase::Hold))
    }

    /// Whether the bar should be mounted at all (any phase but `Hidden`).
    pub fn bar_visible(&self) -> Signal<bool> {
        let phase = self.phase;
        Signal::derive(move || phase.get() != Phase::Hidden)
    }
}
