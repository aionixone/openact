//! Timeout management demonstration and examples

#[cfg(test)]
mod timeout_demo {
    use crate::http::timeout_manager::TimeoutManager;
    use crate::http::connection::TimeoutConfig;
    use std::time::Duration;
    use tokio::time::sleep;

    /// Demonstrates proper timeout configuration for different scenarios
    #[tokio::test]
    async fn demo_timeout_configurations() {
        println!("\n=== Timeout Management Demo ===\n");

        // 1. API calls - balanced timeouts for most API interactions
        let api_timeout = TimeoutManager::for_api_calls();
        println!("📡 API Calls Configuration:");
        println!("  Connect: {}ms", api_timeout.get_config().connect_ms);
        println!("  Read: {}ms", api_timeout.get_config().read_ms);
        println!("  Total: {}ms", api_timeout.get_config().total_ms);
        println!("  Request timeout selected: {}ms\n", 
                 api_timeout.get_request_timeout().as_millis());

        // 2. Health checks - quick timeouts for monitoring
        let health_timeout = TimeoutManager::for_health_check();
        println!("🏥 Health Check Configuration:");
        println!("  Connect: {}ms", health_timeout.get_config().connect_ms);
        println!("  Read: {}ms", health_timeout.get_config().read_ms);
        println!("  Total: {}ms", health_timeout.get_config().total_ms);
        println!("  Request timeout selected: {}ms\n", 
                 health_timeout.get_request_timeout().as_millis());

        // 3. Long operations - generous timeouts for file uploads, etc.
        let long_timeout = TimeoutManager::for_long_operations();
        println!("🐌 Long Operations Configuration:");
        println!("  Connect: {}ms", long_timeout.get_config().connect_ms);
        println!("  Read: {}ms", long_timeout.get_config().read_ms);
        println!("  Total: {}ms", long_timeout.get_config().total_ms);
        println!("  Request timeout selected: {}ms\n", 
                 long_timeout.get_request_timeout().as_millis());

        // All configurations should be valid
        assert!(api_timeout.validate().is_ok());
        assert!(health_timeout.validate().is_ok());
        assert!(long_timeout.validate().is_ok());
    }

    /// Demonstrates timeout behavior with mock operations
    #[tokio::test]
    async fn demo_timeout_execution() {
        println!("\n=== Timeout Execution Demo ===\n");

        // Create a timeout manager with short timeouts for testing
        let timeout_config = TimeoutConfig {
            connect_ms: 1000,
            read_ms: 200,    // Very short for demo
            total_ms: 300,
        };
        let timeout_manager = TimeoutManager::new(timeout_config);

        // 1. Fast operation that succeeds
        println!("⚡ Testing fast operation (50ms)...");
        let fast_result = timeout_manager.execute_with_timeout(async {
            sleep(Duration::from_millis(50)).await;
            Ok::<String, reqwest::Error>("Fast operation completed".to_string())
        }).await;
        
        let fast_success = fast_result.is_ok();
        match fast_result {
            Ok(result) => println!("  ✅ Success: {}", result),
            Err(e) => println!("  ❌ Error: {}", e),
        }

        // 2. Slow operation that times out
        println!("\n🐌 Testing slow operation (400ms, should timeout)...");
        let slow_result = timeout_manager.execute_with_timeout(async {
            sleep(Duration::from_millis(400)).await;
            Ok::<String, reqwest::Error>("Slow operation completed".to_string())
        }).await;
        
        let slow_timeout = slow_result.is_err();
        match slow_result {
            Ok(result) => println!("  ✅ Success: {}", result),
            Err(e) => println!("  ⏰ Expected timeout: {}", e),
        }

        // The fast operation should succeed and slow should timeout
        assert!(fast_success);
        assert!(slow_timeout);
        
        println!("\n=== Demo Complete ===\n");
    }

    /// Demonstrates timeout selection logic
    #[tokio::test]
    async fn demo_timeout_selection() {
        println!("\n=== Timeout Selection Demo ===\n");

        // Case 1: Read timeout is reasonable and less than total
        let config1 = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 10000,
            total_ms: 15000,
        };
        let manager1 = TimeoutManager::new(config1);
        println!("📖 Case 1: read_ms (10s) < total_ms (15s)");
        println!("  Selected timeout: {}ms (uses read_ms)", 
                 manager1.get_request_timeout().as_millis());

        // Case 2: Read timeout is 0 (disabled)
        let config2 = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 0,
            total_ms: 15000,
        };
        let manager2 = TimeoutManager::new(config2);
        println!("\n📖 Case 2: read_ms is 0 (disabled)");
        println!("  Selected timeout: {}ms (uses total_ms)", 
                 manager2.get_request_timeout().as_millis());

        // Case 3: Read timeout is greater than total (invalid)
        let config3 = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 20000,
            total_ms: 15000,
        };
        let manager3 = TimeoutManager::new(config3);
        println!("\n📖 Case 3: read_ms (20s) > total_ms (15s) - invalid");
        println!("  Selected timeout: {}ms (falls back to total_ms)", 
                 manager3.get_request_timeout().as_millis());

        // Verify timeout selection logic
        assert_eq!(manager1.get_request_timeout(), Duration::from_millis(10000));
        assert_eq!(manager2.get_request_timeout(), Duration::from_millis(15000));
        assert_eq!(manager3.get_request_timeout(), Duration::from_millis(15000));
        
        println!("\n=== Selection Demo Complete ===\n");
    }

    /// Demonstrates validation of timeout configurations
    #[tokio::test]
    async fn demo_timeout_validation() {
        println!("\n=== Timeout Validation Demo ===\n");

        // Valid configuration
        let valid_config = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 10000,
            total_ms: 15000,
        };
        let valid_manager = TimeoutManager::new(valid_config);
        println!("✅ Valid config: connect=5s, read=10s, total=15s");
        println!("  Validation result: {:?}", valid_manager.validate());

        // Invalid: connect timeout is 0
        let invalid_config1 = TimeoutConfig {
            connect_ms: 0,
            read_ms: 10000,
            total_ms: 15000,
        };
        let invalid_manager1 = TimeoutManager::new(invalid_config1);
        println!("\n❌ Invalid config 1: connect=0");
        println!("  Validation result: {:?}", invalid_manager1.validate());

        // Invalid: connect > total
        let invalid_config2 = TimeoutConfig {
            connect_ms: 20000,
            read_ms: 10000,
            total_ms: 15000,
        };
        let invalid_manager2 = TimeoutManager::new(invalid_config2);
        println!("\n❌ Invalid config 2: connect=20s > total=15s");
        println!("  Validation result: {:?}", invalid_manager2.validate());

        // Invalid: read > total
        let invalid_config3 = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 20000,
            total_ms: 15000,
        };
        let invalid_manager3 = TimeoutManager::new(invalid_config3);
        println!("\n❌ Invalid config 3: read=20s > total=15s");
        println!("  Validation result: {:?}", invalid_manager3.validate());

        // Verify validation results
        assert!(valid_manager.validate().is_ok());
        assert!(invalid_manager1.validate().is_err());
        assert!(invalid_manager2.validate().is_err());
        assert!(invalid_manager3.validate().is_err());
        
        println!("\n=== Validation Demo Complete ===\n");
    }
}
