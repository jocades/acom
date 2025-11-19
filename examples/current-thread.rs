use std::time::Duration;

use tracing::debug;

use acom::{Executor, future, setup_logging, sleep, spawn};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

pub fn main() {
    setup_logging();

    let exec = Executor::new();

    let f1 = async {
        sleep(ms(1000)).await;
        1
    };

    let f2 = async {
        sleep(ms(3000)).await;
        2
    };

    spawn(async move {
        debug!("start");
        let ret = future::join(f1, f2).await;
        debug!(?ret);

        let ret = future::select(
            async {
                sleep(ms(2000)).await;
                debug!("f1 done");
                3
            },
            async {
                sleep(ms(1000)).await;
                debug!("f2 done");
                4
            },
        )
        .await;
        debug!(?ret);
        debug!("end");
    });

    exec.run();
}
