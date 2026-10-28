use super::*;
use std::collections::VecDeque;

const GB: u64 = 1024 * MB;

fn load(cpu: f32, free_gb: u64) -> Option<Load> {
    Some(Load { cpu_percent: cpu, available_memory: free_gb * GB })
}

fn ok(load: Option<Load>, running: u32) -> Result<(), Hold> {
    admit(load, &Limits::default(), running, Gate::default()).0
}

#[test]
fn a_quiet_machine_admits_up_to_the_hard_limit() {
    assert_eq!(ok(load(10.0, 8), 1), Ok(()));
    assert_eq!(ok(load(10.0, 8), 2), Ok(()));
    assert_eq!(ok(load(10.0, 8), 3), Err(Hold::MaxRuns));
    assert_eq!(ok(None, 3), Err(Hold::MaxRuns), "the hard limit holds without a measurement");
    assert_eq!(ok(None, 2), Ok(()));
}

#[test]
fn busy_cpu_or_little_memory_holds_new_runs() {
    assert_eq!(ok(load(74.9, 8), 1), Ok(()));
    assert_eq!(ok(load(75.0, 8), 1), Err(Hold::Cpu));
    assert_eq!(ok(load(10.0, 2), 1), Ok(()));
    assert_eq!(ok(Some(Load { cpu_percent: 10.0, available_memory: 2 * GB - 1 }), 1), Err(Hold::Memory));
}

#[test]
fn one_run_always_starts_unless_the_limit_is_zero() {
    assert_eq!(ok(load(100.0, 0), 0), Ok(()));
    let none = Limits { max_runs: 1, ..Limits::default() };
    assert_eq!(admit(load(100.0, 0), &none, 0, Gate { cpu_hot: true }).0, Ok(()));
    assert_eq!(admit(load(10.0, 8), &none, 1, Gate::default()).0, Err(Hold::MaxRuns));
    let zero = Limits { max_runs: 0, ..Limits::default() };
    assert_eq!(admit(None, &zero, 0, Gate::default()).0, Err(Hold::DisabledAll));
}

#[test]
fn cpu_hold_lifts_only_below_the_low_mark() {
    let limits = Limits::default();
    let (verdict, gate) = admit(load(80.0, 8), &limits, 1, Gate::default());
    assert_eq!((verdict, gate.cpu_hot), (Err(Hold::Cpu), true));
    // Between the marks: still held.
    let (verdict, gate) = admit(load(70.0, 8), &limits, 1, gate);
    assert_eq!((verdict, gate.cpu_hot), (Err(Hold::Cpu), true));
    let (verdict, gate) = admit(load(64.9, 8), &limits, 1, gate);
    assert_eq!((verdict, gate.cpu_hot), (Ok(()), false));
    // Coming from below, 70 % is fine.
    assert_eq!(admit(load(70.0, 8), &limits, 1, gate).0, Ok(()));
    // The gate also follows the load while nothing waits on it.
    let (_, gate) = admit(load(90.0, 8), &limits, 0, Gate::default());
    assert!(gate.cpu_hot);
}

#[test]
fn limits_come_from_the_environment() {
    let vars = |pairs: &'static [(&'static str, &'static str)]| move |key: &str| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string());
    assert_eq!(Limits::from_vars(|_| None), Limits::default());
    let l = Limits::from_vars(vars(&[("AURELIUS_MAX_RUNS", "5"), ("AURELIUS_CPU_HIGH", "90"), ("AURELIUS_CPU_LOW", "80"), ("AURELIUS_MIN_FREE_MB", "512"), ("AURELIUS_LOAD_ADMISSION", "on")]));
    assert_eq!(l, Limits { max_runs: 5, cpu_high: 90.0, cpu_low: 80.0, min_available_memory: 512 * MB, measured: true });
    // Nonsense falls back; the low mark never exceeds the high one; at least one run.
    let l = Limits::from_vars(vars(&[("AURELIUS_MAX_RUNS", "0"), ("AURELIUS_CPU_HIGH", "50"), ("AURELIUS_CPU_LOW", "x"), ("AURELIUS_LOAD_ADMISSION", "off")]));
    assert_eq!((l.max_runs, l.cpu_high, l.cpu_low, l.measured), (1, 50.0, 50.0, false));
}

#[test]
fn what_the_user_set_wins_per_limit() {
    let env = Limits { max_runs: 5, ..Limits::default() };
    let set = LoadLimitsUpdate { cpu_high: Some(90), mem_min_mb: Some(512), ..Default::default() };
    let l = env.with(&set);
    assert_eq!((l.max_runs, l.cpu_high, l.cpu_low, l.min_available_memory), (5, 90.0, 65.0, 512 * MB), "the environment keeps what the user left alone");
    let view = env.with(&LoadLimitsUpdate { max_runs: Some(0), enabled: Some(false), ..Default::default() }).view();
    assert_eq!(view, LoadLimits { max_runs: 0, cpu_high: 75, cpu_low: 65, mem_min_mb: 2048, enabled: false });
}

#[test]
fn nonsense_cpu_marks_are_refused() {
    let check = |update: LoadLimitsUpdate| validate(&update, &Limits::default().with(&update));
    assert!(check(LoadLimitsUpdate { cpu_high: Some(80), cpu_low: Some(70), ..Default::default() }).is_ok());
    assert!(check(LoadLimitsUpdate { cpu_high: Some(101), ..Default::default() }).is_err());
    assert!(check(LoadLimitsUpdate { cpu_low: Some(75), ..Default::default() }).is_err(), "low must stay below high");
    assert!(check(LoadLimitsUpdate { cpu_high: Some(60), ..Default::default() }).is_err(), "high below the current low");
    assert!(check(LoadLimitsUpdate { max_runs: Some(0), ..Default::default() }).is_ok());
}

#[test]
fn the_meter_averages_its_window() {
    let samples: VecDeque<Load> = [Load { cpu_percent: 20.0, available_memory: 4 * GB }, Load { cpu_percent: 60.0, available_memory: 2 * GB }].into();
    assert_eq!(average(&samples), Some(Load { cpu_percent: 40.0, available_memory: 3 * GB }));
    assert_eq!(average(&VecDeque::new()), None);
}
