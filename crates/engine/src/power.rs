//! Keep the computer awake while the batch queue runs (T8), and give it back its normal sleep settings when the
//! queue ends. Windows ties the request to the thread that makes it, so the queue's own thread holds the guard.

/// While this lives, Windows does not put the computer to sleep (the screen may still turn off).
pub struct KeepAwake(());

impl KeepAwake {
    pub fn new() -> KeepAwake {
        #[cfg(windows)]
        unsafe {
            use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        }
        KeepAwake(())
    }
}

impl Default for KeepAwake {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS};
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}
