//! cab-srv service install / start / stop across user and system scopes.

mod install;
mod runtime;
mod scope;

pub use install::{install_service, uninstall_service};
pub use runtime::{is_service_active, show_logs, start_daemon, stop_daemon};
pub use scope::{ServiceScope, apply_installed_cab_home, load_service_config};

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScopeArg {
    User,
    System,
}

impl From<ScopeArg> for ServiceScope {
    fn from(value: ScopeArg) -> Self {
        match value {
            ScopeArg::User => ServiceScope::User,
            ScopeArg::System => ServiceScope::System,
        }
    }
}
