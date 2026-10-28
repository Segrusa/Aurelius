//! What is running right now, for "2m 10s · 3.9k tokens": per job, when it
//! started and the output tokens its runs wrote so far. A chat answer is one
//! run; a task adds up its steps (planning, workers, review). In memory only:
//! the durable numbers are the usage records.

use aurelius_domain::RunId;
use std::collections::HashMap;
use std::sync::Mutex;

struct JobLive {
    started_at_ms: i64,
    /// Tokens of the job's runs that already ended.
    done: u64,
    /// Tokens of the job's runs that are still going.
    runs: HashMap<RunId, u64>,
    /// A run of the job compacts the conversation's context.
    compacting: bool,
}

#[derive(Default)]
pub struct Live {
    jobs: Mutex<HashMap<String, JobLive>>,
    run_job: Mutex<HashMap<RunId, String>>,
}

impl Live {
    /// A run of this job starts (the job's clock starts with its first run).
    pub fn run_started(&self, job: &str, run: RunId) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.entry(job.to_owned())
                .or_insert_with(|| JobLive { started_at_ms: aurelius_storage::now_ms(), done: 0, runs: HashMap::new(), compacting: false })
                .runs
                .insert(run, 0);
        }
        if let Ok(mut map) = self.run_job.lock() {
            map.insert(run, job.to_owned());
        }
    }

    /// Output tokens of a run so far. Never goes down: an estimate from the
    /// text and the runtime's own count may both come in.
    pub fn tokens(&self, run: RunId, total: u64) {
        let Some(job) = self.run_job.lock().ok().and_then(|m| m.get(&run).cloned()) else { return };
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(n) = jobs.get_mut(&job).and_then(|j| j.runs.get_mut(&run)) {
                *n = (*n).max(total);
            }
        }
    }

    /// A run ended; its tokens stay counted for the job.
    pub fn run_ended(&self, run: RunId) {
        let Some(job) = self.run_job.lock().ok().and_then(|mut m| m.remove(&run)) else { return };
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(j) = jobs.get_mut(&job) {
                let n = j.runs.remove(&run).unwrap_or(0);
                j.done += n;
            }
        }
    }

    /// The job is over (or handed back): forget it.
    pub fn job_ended(&self, job: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(j) = jobs.remove(job) {
                if let Ok(mut map) = self.run_job.lock() {
                    for run in j.runs.keys() {
                        map.remove(run);
                    }
                }
            }
        }
    }

    /// (started at, output tokens so far) of a job.
    pub fn job(&self, job: &str) -> Option<(i64, u64)> {
        let jobs = self.jobs.lock().ok()?;
        let j = jobs.get(job)?;
        Some((j.started_at_ms, j.done + j.runs.values().sum::<u64>()))
    }

    /// The run starts or stops compacting the conversation's context.
    pub fn set_compacting(&self, run: RunId, on: bool) {
        let Some(job) = self.run_job.lock().ok().and_then(|m| m.get(&run).cloned()) else { return };
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(j) = jobs.get_mut(&job) {
                j.compacting = on;
            }
        }
    }

    /// Whether the run's job compacts the conversation's context now.
    pub fn run_compacting(&self, run: RunId) -> bool {
        self.run_job.lock().ok().and_then(|m| m.get(&run).cloned()).is_some_and(|job| self.is_compacting(&job))
    }

    /// Whether the job compacts the conversation's context now.
    pub fn is_compacting(&self, job: &str) -> bool {
        self.jobs.lock().ok().and_then(|jobs| jobs.get(job).map(|j| j.compacting)).unwrap_or(false)
    }

    /// The same for the job a run belongs to.
    pub fn for_run(&self, run: RunId) -> Option<(i64, u64)> {
        let job = self.run_job.lock().ok()?.get(&run).cloned()?;
        self.job(&job)
    }
}

#[cfg(test)]
mod tests {
    use super::Live;
    use aurelius_domain::RunId;

    #[test]
    fn a_task_adds_up_its_steps_and_a_chat_is_one_run() {
        let live = Live::default();
        let (a, b) = (RunId::new(), RunId::new());
        live.run_started("task-1", a);
        live.run_started("task-1", b);
        live.tokens(a, 100);
        live.tokens(a, 40); // an estimate behind the real count: ignored
        live.tokens(b, 50);
        assert_eq!(live.job("task-1").unwrap().1, 150);
        live.run_ended(a);
        assert_eq!(live.for_run(b).unwrap().1, 150, "ended steps stay counted");
        live.tokens(a, 999); // after it ended: no longer counted
        assert_eq!(live.job("task-1").unwrap().1, 150);
        live.set_compacting(b, true);
        assert!(live.is_compacting("task-1"));
        live.set_compacting(b, false);
        assert!(!live.is_compacting("task-1"));
        live.job_ended("task-1");
        assert!(live.job("task-1").is_none() && live.for_run(b).is_none());
    }
}
