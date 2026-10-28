//! Process tree of a run. On Windows, killing `claude.exe` leaves its own
//! children (the `aurelius-ctl` MCP bridge, shells) running, so every run's
//! child goes into a job object that kills the whole tree when it is
//! terminated or dropped. Elsewhere this is a no-op.

#[cfg(windows)]
pub struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for ProcessTree {}
#[cfg(windows)]
unsafe impl Sync for ProcessTree {}

#[cfg(windows)]
impl ProcessTree {
    /// Puts the child in a new kill-on-close job. `None` if that fails (the run
    /// still works; only its grandchildren may outlive a stop).
    pub fn adopt(child: &tokio::process::Child) -> Option<Self> {
        use windows_sys::Win32::System::JobObjects::*;
        let process = child.raw_handle()?;
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let tree = ProcessTree(job);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 || AssignProcessToJobObject(job, process as _) == 0 {
                tracing::warn!("could not put Claude Code in a job object; its children may outlive a stop");
                return None;
            }
            Some(tree)
        }
    }

    /// Kills every process still in the tree.
    pub fn kill(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        // KILL_ON_JOB_CLOSE: closing the last handle ends what is left.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::ProcessTree;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    /// `ping` inherits cmd's stdout: the pipe only closes once the grandchild is gone too.
    #[tokio::test]
    async fn killing_the_tree_ends_grandchildren() {
        let mut child = tokio::process::Command::new("cmd")
            .args(["/c", "ping -n 60 127.0.0.1 > nul & ping -n 60 127.0.0.1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let tree = ProcessTree::adopt(&child).expect("job object");
        let mut stdout = child.stdout.take().unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        tree.kill();
        let mut buf = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), stdout.read_to_end(&mut buf)).await.expect("grandchild still holds stdout").unwrap();
        let _ = child.wait().await;
    }
}

#[cfg(not(windows))]
pub struct ProcessTree;

#[cfg(not(windows))]
impl ProcessTree {
    pub fn adopt(_child: &tokio::process::Child) -> Option<Self> {
        None
    }

    pub fn kill(&self) {}
}
