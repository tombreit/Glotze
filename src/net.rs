//! Shared HTTP setup. One place for the user-agent and the ureq `Agent`
//! construction used by both the search API and the downloader. TLS is ureq's
//! default (rustls + ring + bundled Mozilla roots), so there is nothing to wire.

use std::time::Duration;

use ureq::Agent;

const USER_AGENT: &str = concat!("Glotze/", env!("CARGO_PKG_VERSION"));

/// Build an `Agent` with our user-agent and the given timeouts.
///
/// `global` caps the whole request — pass `None` for downloads, which can run
/// arbitrarily long. `connect` is the TCP connect timeout.
pub fn agent(global: Option<Duration>, connect: Option<Duration>) -> Agent {
    let config = Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_global(global)
        .timeout_connect(connect)
        .build();
    Agent::new_with_config(config)
}
