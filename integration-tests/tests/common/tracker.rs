#![allow(dead_code, unused_variables)]

use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ExecutionOrder {
    events: Arc<Mutex<Vec<String>>>,
}

impl ExecutionOrder {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn track(&self, event: impl Into<String>) {
        self.events.lock().unwrap().push(event.into());
    }

    pub fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    pub fn assert_contains(&self, event: &str) {
        let events = self.events();
        assert!(
            events.iter().any(|e| e == event),
            "Expected '{}' in events, got: {:?}",
            event,
            events
        );
    }

    pub fn assert_order(&self, expected: &[&str]) {
        let events = self.events();
        for (i, exp) in expected.iter().enumerate() {
            assert_eq!(
                events.get(i).map(|s| s.as_str()),
                Some(*exp),
                "Expected '{}' at position {}, got {:?}",
                exp,
                i,
                events.get(i)
            );
        }
    }

    pub fn assert_not_contains(&self, event: &str) {
        let events = self.events();
        assert!(
            !events.iter().any(|e| e == event),
            "'{}' should not be in events, got: {:?}",
            event,
            events
        );
    }
}
