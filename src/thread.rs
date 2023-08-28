use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::thread;

use tokio::runtime::Handle;
use tokio::task::LocalSet;
use tokio::sync::oneshot;

pub async fn spawn_worker<T: Send + 'static>(
    name: &str,
    fut: impl Future<Output = T> + Send + 'static,
) -> T {
    let runtime = Handle::current();
    let (tx, rx) = oneshot::channel();

    // we can't use tokio's own spawn_blocking function because it has no
    // facility to name the new thread. do it the manual way instead
    let result = thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let _runtime_guard = runtime.enter();

                let local = LocalSet::new();
                let local_fut = local.run_until(fut);
                runtime.block_on(local_fut)
            }));

            let _ = tx.send(result);
        });

    match result {
        Ok(_) => {}
        Err(err) => { panic!("error spawning thread {}: {:?}", name, err); }
    }

    match rx.await {
        Ok(Ok(val)) => val,
        Ok(Err(panic)) => {
            panic!("thread {} panicked: {:?}", name, panic);
        }
        Err(_) => {
            panic!("thread {} unexpectedly terminated", name);
        }
    }
}
