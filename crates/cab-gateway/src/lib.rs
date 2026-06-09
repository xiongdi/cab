#![allow(clippy::all, dead_code)]
pub mod agent_id;
pub mod anthropic;
pub mod fallback;
pub mod openai;
pub mod protocol;
pub mod proxy;
pub mod router;
pub mod server;
pub mod state;

pub use server::gateway_router;
pub use state::GatewayState;
