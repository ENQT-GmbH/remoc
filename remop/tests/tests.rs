use std::sync::Once;

mod chmux;

static INIT: Once = Once::new();

pub fn init() {
    INIT.call_once(env_logger::init);
}
