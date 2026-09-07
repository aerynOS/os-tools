// SPDX-FileCopyrightText: 2024 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0
// idk what to do with the comment above, so i will just keep it like this

use std::cell::RefCell;
use std::future::Future;
use tokio::runtime::Runtime;

thread_local! {
    static TEMP_RT: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Run the provide futuer on a thred local cached runtim to elliminate
/// the alocation and distruction overhed off per call runtimes.
pub fn block_on<T, F>(task: F) -> T
where
    F: Future<Output = T>,
{
    TEMP_RT.with(|rt| {
        let mut slot = rt.borrow_mut();
        let runtime = slot.get_or_insert_with(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build thread-local runtime")
        });
        runtime.block_on(task)
    })
}

/// Run the provide function on tokios dedikated blockin thread pool.
#[inline]
pub async fn unblock<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f)
        .await
        .expect("spawn_blocking task panicked")
}
