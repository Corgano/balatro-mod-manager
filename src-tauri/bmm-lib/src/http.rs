//! Shared `reqwest::Client` instances.
//!
//! Creating a new `reqwest::Client` per call leaks the underlying connection
//! pool, DNS resolver, and TLS context. Almost every callsite in this crate
//! only needs a generic client (or a client with redirects disabled), so a
//! pair of process-wide singletons is plenty.

use once_cell::sync::OnceCell;
use reqwest::Client;
use std::time::Duration;

static DEFAULT_CLIENT: OnceCell<Client> = OnceCell::new();
static NO_REDIRECT_CLIENT: OnceCell<Client> = OnceCell::new();

/// Returns a shared `reqwest::Client` configured with the project's user
/// agent and sensible timeouts. The first call lazily builds the client;
/// every subsequent call returns the same instance, so the connection pool
/// is reused across mod downloads, Lovely fetches, etc.
///
/// Falls back to `Client::new()` if the builder fails (extremely unlikely);
/// the returned client is fresh for that call only, but reuse resumes
/// immediately afterward.
pub fn shared_client() -> Client {
    DEFAULT_CLIENT
        .get_or_init(|| {
            Client::builder()
                .user_agent("balatro-mod-manager")
                .pool_idle_timeout(Some(Duration::from_secs(90)))
                .build()
                .unwrap_or_else(|e| {
                    log::warn!("Failed to build shared reqwest client: {e}; using default");
                    Client::new()
                })
        })
        .clone()
}

/// Shared client with redirects disabled. Used by the Lovely version
/// resolver, which inspects the `Location` header of GitHub's latest-release
/// endpoint.
pub fn shared_no_redirect_client() -> Client {
    NO_REDIRECT_CLIENT
        .get_or_init(|| {
            Client::builder()
                .user_agent("balatro-mod-manager")
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|e| {
                    log::warn!(
                        "Failed to build shared no-redirect reqwest client: {e}; using default"
                    );
                    Client::new()
                })
        })
        .clone()
}
