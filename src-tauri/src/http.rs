//! The one HTTP client every Groq call shares.
//!
//! Building a `reqwest::Client` per request meant a fresh TCP connection and a
//! fresh TLS handshake to `api.groq.com` on every single call — twice per
//! dictation, since transcription and grammar are two separate requests.
//!
//! Measured on the sibling app, which had exactly this shape: the two handshakes
//! were 250 ms of a 964 ms dictation, and removing them cut the median to 715 ms.
//! The consistency gain was larger than the mean gain — total std dev fell from
//! 203 ms to 75 ms — because a TLS negotiation going badly is far more variable
//! than inference, so the handshake was the slow tail rather than a flat tax.
//!
//! A shared client keeps the connection in a pool and reuses it, so the second
//! call of a dictation — and every call of the next one — skips the handshake.
//!
//! The pool is only half of it. `reqwest`'s default idle timeout would drop the
//! connection long before the next dictation on a machine someone dictates into
//! a few times an hour, which is exactly the pattern this app has. Hence the long
//! idle window below, and `warm` for the case where it has lapsed anyway.
//!
//! One consequence for callers: the timeout cannot live on the client any more,
//! because the two callers want very different ones (60 s for an upload, 2 s for
//! grammar). Both set `.timeout()` per request instead, which `reqwest` supports
//! directly and which leaves their behaviour unchanged.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;
use tauri::AppHandle;

/// How long an unused connection is kept.
///
/// Dictations come in bursts minutes apart, so the default 90 s would leave most
/// of them paying for a new handshake. Groq's own server-side idle timeout is
/// what actually closes these; a generous value here just means we stop
/// discarding a connection that is still good.
const POOL_IDLE: Duration = Duration::from_secs(600);

/// Sent on the idle connection so a NAT or firewall in the middle does not
/// silently drop it and leave us reusing a socket that is already dead.
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);

/// How long establishing the TCP + TLS connection may take.
///
/// Separate from the per-request timeouts, and much shorter than the upload's: a
/// host that cannot be reached at all should say so quickly rather than consume
/// the whole request budget before admitting it. Measured handshakes to Groq are
/// well under 500 ms, so this only fires when something is genuinely wrong.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Cheap authenticated GET, used only to establish the connection.
const WARM_ENDPOINT: &str = "https://api.groq.com/openai/v1/models";

/// How long the warm-up may take before it gives up. It is entirely optional
/// work — nothing waits on it — so it must not linger.
const WARM_TIMEOUT: Duration = Duration::from_secs(5);

/// Built once, on first use.
///
/// A `OnceLock` rather than Tauri managed state because neither `groq` nor
/// `grammar` is handed an `AppHandle`, and threading one through both purely to
/// reach the client would change signatures for no gain. The client is
/// process-global either way.
static CLIENT: OnceLock<Client> = OnceLock::new();

/// The shared client.
///
/// `Client` is internally reference-counted, so handing out clones is how it is
/// meant to be used — every clone shares the same connection pool.
pub fn client() -> Client {
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .pool_idle_timeout(POOL_IDLE)
                // Only one host (api.groq.com), so this needs to be just enough
                // to hold the one connection open across calls.
                .pool_max_idle_per_host(4)
                .tcp_keepalive(TCP_KEEPALIVE)
                .connect_timeout(CONNECT_TIMEOUT)
                // Deliberately no `.timeout()`: the upload wants 60 s and
                // grammar wants 2 s, so each sets its own per request.
                .build()
                // A builder failure here means the TLS backend could not start,
                // which would break every request either way. `Client::new()`
                // panics on the same condition, so behaviour is unchanged — the
                // fallback just keeps this from being the line that panics.
                .unwrap_or_else(|err| {
                    eprintln!("piplo: could not build the shared HTTP client ({err}) — using defaults");
                    Client::new()
                })
        })
        .clone()
}

/// Open the connection to Groq now, so the dictation about to happen does not
/// have to.
///
/// Called when recording starts. The user then speaks for a few seconds, which is
/// far longer than a handshake takes, so by the time there is a WAV to upload the
/// pooled connection is already established and the transcription request goes
/// straight out on it.
///
/// Deliberately fire-and-forget: no status, no error surfaced, nothing waits on
/// it. If it fails, the transcription request opens its own connection exactly as
/// it did before, and the user sees no difference beyond the latency.
pub fn warm(app: &AppHandle) {
    let Some(key) = crate::credentials::current(app) else {
        return;
    };
    if key.is_empty() {
        return;
    }

    let client = client();
    tauri::async_runtime::spawn(async move {
        let _ = client
            .get(WARM_ENDPOINT)
            .bearer_auth(key)
            .timeout(WARM_TIMEOUT)
            .send()
            .await;
    });
}
