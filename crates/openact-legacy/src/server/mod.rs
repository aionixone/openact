#![cfg(feature = "server")]

pub mod router;
pub mod handlers {
    pub mod connect;
    pub mod connections;
    pub mod execute;
    pub mod system;
    pub mod tasks;

    #[cfg(test)]
    mod tests;
}
pub mod authflow;

#[cfg(feature = "server")]
pub fn init_background_tasks() {
    // Spawn AC result TTL cleaner
    crate::server::handlers::connect::spawn_ac_ttl_cleaner();
}
