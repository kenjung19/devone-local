//! Cooperative cancellation for an unlocked IPC worker, scoped to its thread.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
thread_local! { static CANCEL: std::cell::RefCell<Option<Arc<AtomicBool>>> = const { std::cell::RefCell::new(None) }; }
pub(crate) fn set(cancel: Option<Arc<AtomicBool>>) {
    CANCEL.with(|flag| *flag.borrow_mut() = cancel);
}
pub(crate) fn cancelled() -> bool {
    CANCEL.with(|flag| {
        flag.borrow()
            .as_ref()
            .is_some_and(|f| f.load(Ordering::Acquire))
    })
}
pub(crate) fn check() -> crate::core::Result<()> {
    if cancelled() {
        return Err(crate::core::Error::Cancelled(
            "Operation cancelled while DEVONE is quitting".into(),
        ));
    }
    Ok(())
}
