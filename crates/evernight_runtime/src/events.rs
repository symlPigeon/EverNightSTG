use evernight_core::EventPayload;

#[derive(Default)]
pub struct EventBus {
    queue: Vec<EventPayload>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes an event into the queue for this frame.
    pub fn push(&mut self, event: EventPayload) {
        self.queue.push(event);
    }

    /// Returns a read-only slice of all events queued this frame.
    pub fn events(&self) -> &[EventPayload] {
        &self.queue
    }

    /// Clears all events. Called by the scheduler at the end of each frame.
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}
