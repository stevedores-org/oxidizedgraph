//! Event bus implementation using tokio broadcast channels

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

use super::types::Event;

/// Default channel capacity
const DEFAULT_CAPACITY: usize = 1024;

/// Event bus for publishing and subscribing to graph events
///
/// Uses tokio broadcast channels for efficient fan-out to multiple subscribers.
/// Events are cloned to each subscriber, so subscribers can process independently.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
    subscriber_count: Arc<AtomicUsize>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Create a new event bus with default capacity
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new event bus with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            subscriber_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Publish an event to all subscribers
    ///
    /// Returns the number of subscribers that received the event.
    /// If there are no subscribers, the event is dropped.
    pub fn publish(&self, event: Event) -> usize {
        match self.sender.send(event) {
            Ok(count) => count,
            Err(_) => 0, // No active receivers
        }
    }

    /// Subscribe to events
    ///
    /// Returns an `EventReceiver` that can be used to receive events.
    /// Multiple subscribers can exist simultaneously.
    pub fn subscribe(&self) -> EventReceiver {
        self.subscriber_count.fetch_add(1, Ordering::SeqCst);
        EventReceiver {
            receiver: self.sender.subscribe(),
            subscriber_count: self.subscriber_count.clone(),
        }
    }

    /// Get the current number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.subscriber_count.load(Ordering::SeqCst)
    }

    /// Check if there are any active subscribers
    pub fn has_subscribers(&self) -> bool {
        self.subscriber_count() > 0
    }

    /// Create a subscription handle that automatically unsubscribes on drop
    pub fn subscription(&self) -> EventSubscription {
        EventSubscription {
            receiver: Some(self.subscribe()),
        }
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("subscriber_count", &self.subscriber_count())
            .finish()
    }
}

/// Receiver for events from an EventBus
pub struct EventReceiver {
    receiver: broadcast::Receiver<Event>,
    subscriber_count: Arc<AtomicUsize>,
}

impl EventReceiver {
    /// Receive the next event
    ///
    /// Returns `None` if the channel is closed.
    /// May skip events if the receiver falls behind (lagged).
    pub async fn recv(&mut self) -> Option<Event> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped = skipped, "Event receiver lagged, some events were dropped");
                    continue; // Try again
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// Try to receive an event without blocking
    ///
    /// Returns `None` if no event is available or channel is closed.
    pub fn try_recv(&mut self) -> Option<Event> {
        match self.receiver.try_recv() {
            Ok(event) => Some(event),
            Err(_) => None,
        }
    }

    /// Receive events as a stream
    #[cfg(feature = "stream")]
    pub fn into_stream(self) -> impl futures::Stream<Item = Event> {
        tokio_stream::wrappers::BroadcastStream::new(self.receiver)
            .filter_map(|result| async move { result.ok() })
    }
}

impl Drop for EventReceiver {
    fn drop(&mut self) {
        self.subscriber_count.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A subscription handle that automatically unsubscribes when dropped
pub struct EventSubscription {
    receiver: Option<EventReceiver>,
}

impl EventSubscription {
    /// Get a reference to the receiver
    pub fn receiver(&mut self) -> Option<&mut EventReceiver> {
        self.receiver.as_mut()
    }

    /// Take ownership of the receiver
    pub fn take_receiver(&mut self) -> Option<EventReceiver> {
        self.receiver.take()
    }

    /// Receive the next event
    pub async fn recv(&mut self) -> Option<Event> {
        if let Some(ref mut receiver) = self.receiver {
            receiver.recv().await
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_subscribe() {
        let bus = EventBus::new();

        let mut receiver = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        let event = Event::graph_started("thread-1", None, "start".to_string());
        let count = bus.publish(event.clone());

        assert_eq!(count, 1);

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.thread_id, "thread-1");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new();

        let mut receiver1 = bus.subscribe();
        let mut receiver2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        let event = Event::node_entered("thread-1", "node-a".to_string(), 1);
        let count = bus.publish(event);

        assert_eq!(count, 2);

        let received1 = receiver1.recv().await.unwrap();
        let received2 = receiver2.recv().await.unwrap();

        assert_eq!(received1.thread_id, received2.thread_id);
    }

    #[tokio::test]
    async fn test_subscriber_drop() {
        let bus = EventBus::new();

        {
            let _receiver = bus.subscribe();
            assert_eq!(bus.subscriber_count(), 1);
        }

        assert_eq!(bus.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn test_no_subscribers() {
        let bus = EventBus::new();

        let event = Event::graph_started("thread-1", None, "start".to_string());
        let count = bus.publish(event);

        assert_eq!(count, 0);
    }

    #[test]
    fn test_event_bus_debug() {
        let bus = EventBus::new();
        let _receiver = bus.subscribe();

        let debug = format!("{:?}", bus);
        assert!(debug.contains("subscriber_count: 1"));
    }
}
