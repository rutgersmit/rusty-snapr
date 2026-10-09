use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

use super::wide;

/// Holds a named mutex for as long as this instance runs.
pub struct SingleInstance(HANDLE);

impl SingleInstance {
    /// Returns `None` when another instance already holds the mutex.
    pub fn acquire(name: &str) -> Option<SingleInstance> {
        let name = wide(name);
        // SAFETY: `name` is a valid null-terminated string for the duration of the call.
        let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.ok()?;
        // SAFETY: reads the calling thread's last-error value, set by CreateMutexW.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: we own this handle and don't use it afterwards.
            unsafe {
                let _ = CloseHandle(handle);
            }
            return None;
        }
        Some(SingleInstance(handle))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateMutexW and is closed exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
