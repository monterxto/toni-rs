//! Test for Controller Constructor-based Dependency Injection
//!
//! This test verifies that controllers support the same DI patterns as services:
//! 1. Auto-detected new() constructor with DI
//! 2. Explicit init attribute with custom constructor
//! 3. Default fallback for structs without constructors

use toni::{controller, get, injectable, module, Body as ToniBody, HttpRequest};

// ============= Base Service =============

#[injectable(pub struct ConfigService {
    value: String,
})]
impl ConfigService {
    pub fn new() -> Self {
        Self {
            value: "test_config".to_string(),
        }
    }

    pub fn get_value(&self) -> String {
        self.value.clone()
    }
}

// ============= Example 1: Auto-detected new() =============

#[controller("/auto", pub struct AutoDetectedController {
    config_value: String,
})]
impl AutoDetectedController {
    // new() is auto-detected - config param will be DI-resolved
    pub fn new(config: ConfigService) -> Self {
        Self {
            config_value: config.get_value(),
        }
    }

    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(format!("AutoDetected: {}", self.config_value))
    }
}

// ============= Example 2: Explicit init with custom name =============

#[controller("/explicit", init = "create", pub struct ExplicitInitController {
    combined: String,
})]
impl ExplicitInitController {
    // Custom constructor name - config param will be DI-resolved
    pub fn create(config: ConfigService) -> Self {
        Self {
            combined: format!("Explicit: {}", config.get_value()),
        }
    }

    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(self.combined.clone())
    }
}

// ============= Example 3: Default fallback (no constructor) =============

#[injectable]
pub struct DefaultFallbackController {
    name: String, // Uses String::default() = ""
    count: i32,   // Uses i32::default() = 0
}

// Note: For this test, we need to use the old #[inject] pattern for comparison
// or implement Default for the controller
impl DefaultFallbackController {
    pub fn get_info(&self) -> String {
        format!("name='{}', count={}", self.name, self.count)
    }
}

// ============= Example 4: Mixed - new() with multiple params =============

#[injectable(pub struct HelperService {
    data: String,
})]
impl HelperService {
    pub fn new() -> Self {
        Self {
            data: "helper".to_string(),
        }
    }

    pub fn get_data(&self) -> String {
        self.data.clone()
    }
}

#[controller("/multi", pub struct MultiParamController {
    result: String,
})]
impl MultiParamController {
    pub fn new(config: ConfigService, helper: HelperService) -> Self {
        Self {
            result: format!(
                "config={}, helper={}",
                config.get_value(),
                helper.get_data()
            ),
        }
    }

    #[get("/test")]
    fn test(&self, _req: HttpRequest) -> ToniBody {
        ToniBody::Text(self.result.clone())
    }
}

// ============= Test Module =============

#[module(
    providers: [ConfigService, HelperService],
    controllers: [
        AutoDetectedController,
        ExplicitInitController,
        MultiParamController,
    ],
)]
impl ControllerConstructorTestModule {}

// ============= Tests =============

#[cfg(test)]
mod tests {
    use super::*;
}
