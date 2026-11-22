use std::time::Duration;

use iced::futures::{Stream, StreamExt};
use zbus::message::Type;
use zbus::proxy::{Builder, CacheProperties};
use zbus::{Connection, MatchRule, MessageStream, Proxy};

// A single change emits a burst of signals; wait this long for it to end before reading once.
const SETTLE: Duration = Duration::from_millis(50);

/// Proxy that reads properties on demand instead of caching them, so it adds no signal subscriptions.
pub async fn proxy<'a>(connection: &Connection, service: &'a str, path: &'a str, interface: &'a str) -> zbus::Result<Proxy<'a>> {
    Builder::new(connection)
        .destination(service)?
        .path(path)?
        .interface(interface)?
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

/// Every signal sent by `service`.
pub async fn signals(connection: &Connection, service: &str) -> zbus::Result<MessageStream> {
    let rule = MatchRule::builder().msg_type(Type::Signal).sender(service)?.build();
    MessageStream::for_match_rule(rule, connection, None).await
}

/// Waits for the next signal and swallows the rest of its burst. Returns false when the stream ends.
pub async fn next_change<S: Stream + Unpin>(signals: &mut S) -> bool {
    if signals.next().await.is_none() {
        return false;
    }
    while let Ok(Some(_)) = tokio::time::timeout(SETTLE, signals.next()).await {}
    true
}
