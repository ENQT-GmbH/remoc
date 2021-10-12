use std::sync::Once;

mod chmux;
mod rsync;

static INIT: Once = Once::new();

pub fn init() {
    INIT.call_once(env_logger::init);
}

#[macro_export]
macro_rules! loop_transport {
    ($queue_length:expr, $a_tx:ident, $a_rx:ident, $b_tx:ident, $b_rx:ident) => {
        let ($a_tx, $b_rx) = futures::channel::mpsc::channel::<Bytes>($queue_length);
        let ($b_tx, $a_rx) = futures::channel::mpsc::channel::<Bytes>($queue_length);

        let $a_rx = $a_rx.map(Ok::<_, std::io::Error>);
        let $b_rx = $b_rx.map(Ok::<_, std::io::Error>);
    };
}
