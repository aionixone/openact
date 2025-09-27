//! Retry management demonstration and examples

#[cfg(test)]
mod retry_demo {
    use crate::error::ConnectorError;
    use crate::http::retry_manager::{RetryManager, ErrorClassification, RetryDecision};
    use crate::http::connection::RetryPolicy;
    use std::time::{Duration, Instant};

    /// Demonstrates error classification for different types of errors
    #[tokio::test]
    async fn demo_error_classification() {
        println!("\n=== Error Classification Demo ===\n");

        let manager = RetryManager::for_api_calls();

        // Test various error types
        let errors = vec![
            ("Timeout Error", ConnectorError::Timeout("Request timed out".to_string())),
            ("Connection Error", ConnectorError::Connection("Connection refused".to_string())),
            ("Authentication Error", ConnectorError::Authentication("Invalid credentials".to_string())),
            ("Invalid Config", ConnectorError::InvalidConfig("Bad URL".to_string())),
            ("Validation Error", ConnectorError::Validation("Missing field".to_string())),
        ];

        for (name, error) in errors {
            let classification = manager.classify_error(&error);
            let should_retry = match classification {
                ErrorClassification::Retryable => "✅ YES",
                ErrorClassification::RateLimited => "⏰ YES (with longer delay)",
                ErrorClassification::NonRetryable => "❌ NO",
            };

            println!("{}: {}", name, should_retry);
            println!("  Error: {}", error);
            println!("  Classification: {:?}\n", classification);
        }

        println!("=== Status Code Classification ===\n");

        let status_codes = vec![
            (200, "OK"),
            (400, "Bad Request"),
            (401, "Unauthorized"),
            (408, "Request Timeout"),
            (429, "Too Many Requests"),
            (500, "Internal Server Error"),
            (502, "Bad Gateway"),
            (503, "Service Unavailable"),
        ];

        for (code, description) in status_codes {
            let classification = manager.classify_status_code(code);
            let should_retry = match classification {
                ErrorClassification::Retryable => "✅ YES",
                ErrorClassification::RateLimited => "⏰ YES (rate limited)",
                ErrorClassification::NonRetryable => "❌ NO",
            };

            println!("{} {}: {}", code, description, should_retry);
        }

        println!("\n=== Classification Demo Complete ===\n");
    }

    /// Demonstrates retry decision making with different scenarios
    #[tokio::test]
    async fn demo_retry_decisions() {
        println!("\n=== Retry Decision Demo ===\n");

        let manager = RetryManager::for_api_calls();
        let start_time = Instant::now();

        // Scenario 1: Retryable error within limits
        println!("🔄 Scenario 1: Retryable error (first attempt)");
        let timeout_error = ConnectorError::Timeout("Network timeout".to_string());
        let decision = manager.should_retry(&timeout_error, 0, start_time);
        
        match decision {
            RetryDecision::Retry { delay, attempt_info } => {
                println!("  ✅ Decision: RETRY");
                println!("  Next attempt: {}", attempt_info.attempt_number);
                println!("  Delay: {}ms", delay.as_millis());
                println!("  Error classification: {:?}", attempt_info.error_classification);
            }
            RetryDecision::Stop { reason, .. } => {
                println!("  ❌ Decision: STOP - {}", reason);
            }
        }

        // Scenario 2: Non-retryable error
        println!("\n🚫 Scenario 2: Non-retryable error");
        let auth_error = ConnectorError::Authentication("Invalid token".to_string());
        let decision = manager.should_retry(&auth_error, 0, start_time);
        
        match decision {
            RetryDecision::Retry { .. } => {
                println!("  ❌ Unexpected: Should not retry auth error");
            }
            RetryDecision::Stop { reason, .. } => {
                println!("  ✅ Decision: STOP - {}", reason);
            }
        }

        // Scenario 3: Max attempts exceeded
        println!("\n🔢 Scenario 3: Max attempts exceeded");
        let decision = manager.should_retry(&timeout_error, 3, start_time); // max_retries = 3
        
        match decision {
            RetryDecision::Retry { .. } => {
                println!("  ❌ Unexpected: Should not retry after max attempts");
            }
            RetryDecision::Stop { reason, .. } => {
                println!("  ✅ Decision: STOP - {}", reason);
            }
        }

        println!("\n=== Decision Demo Complete ===\n");
    }

    /// Demonstrates delay calculation with exponential backoff and jitter
    #[tokio::test]
    async fn demo_delay_calculation() {
        println!("\n=== Delay Calculation Demo ===\n");

        // Create managers with and without jitter for comparison
        let with_jitter = RetryManager::for_api_calls().with_jitter(true);
        let without_jitter = RetryManager::for_api_calls().with_jitter(false);

        println!("📊 Exponential Backoff Pattern:");
        println!("  Policy: initial=1000ms, multiplier=2.0, max=16000ms\n");

        for attempt in 0..5 {
            let delay_no_jitter = without_jitter.calculate_delay(attempt, &ErrorClassification::Retryable);
            let delay_with_jitter = with_jitter.calculate_delay(attempt, &ErrorClassification::Retryable);
            let rate_limit_delay = with_jitter.calculate_delay(attempt, &ErrorClassification::RateLimited);

            println!("  Attempt {}: ", attempt);
            println!("    Without jitter: {}ms", delay_no_jitter.as_millis());
            println!("    With jitter:    {}ms", delay_with_jitter.as_millis());
            println!("    Rate limited:   {}ms", rate_limit_delay.as_millis());
        }

        println!("\n🎲 Jitter Demonstration (10 samples for attempt 1):");
        for i in 1..=10 {
            let delay = with_jitter.calculate_delay(1, &ErrorClassification::Retryable);
            println!("    Sample {}: {}ms", i, delay.as_millis());
        }

        println!("\n=== Delay Demo Complete ===\n");
    }

    /// Demonstrates different retry manager presets
    #[tokio::test]
    async fn demo_retry_presets() {
        println!("\n=== Retry Presets Demo ===\n");

        let presets = vec![
            ("API Calls", RetryManager::for_api_calls()),
            ("Background Jobs", RetryManager::for_background_jobs()),
            ("Health Checks", RetryManager::for_health_checks()),
        ];

        for (name, manager) in presets {
            let policy = manager.get_policy();
            let total_timeout = manager.get_max_total_duration();

            println!("🎯 {}:", name);
            println!("  Max retries: {}", policy.max_retries);
            println!("  Initial delay: {}ms", policy.initial_delay_ms);
            println!("  Max delay: {}ms", policy.max_delay_ms);
            println!("  Backoff multiplier: {}", policy.backoff_multiplier);
            println!("  Total timeout: {}s", total_timeout.as_secs());
            println!("  Jitter enabled: {}", if name == "Health Checks" { false } else { true });
            print!("  Retry status codes: ");
            for (i, code) in policy.retry_on_status_codes.iter().enumerate() {
                if i > 0 { print!(", "); }
                print!("{}", code);
            }
            println!("\n");
        }

        println!("=== Presets Demo Complete ===\n");
    }

    /// Demonstrates complete retry sequence simulation
    #[tokio::test]
    async fn demo_complete_retry_sequence() {
        println!("\n=== Complete Retry Sequence Demo ===\n");

        let manager = RetryManager::for_api_calls();
        let start_time = Instant::now();
        let error = ConnectorError::Timeout("Simulated timeout".to_string());

        println!("🔄 Simulating complete retry sequence for timeout error:\n");

        let mut attempt = 0u32;
        let mut total_delay = Duration::from_millis(0);

        loop {
            match manager.should_retry(&error, attempt, start_time + total_delay) {
                RetryDecision::Retry { delay, attempt_info } => {
                    total_delay += delay;
                    println!("  Attempt {}: Wait {}ms (total elapsed: {}ms)", 
                            attempt_info.attempt_number,
                            delay.as_millis(), 
                            total_delay.as_millis());
                    attempt = attempt_info.attempt_number;
                }
                RetryDecision::Stop { reason, final_attempt, total_elapsed } => {
                    println!("\n  ⏹️  Stopped after {} attempts", final_attempt);
                    println!("  Reason: {}", reason);
                    println!("  Total time: {}ms", total_elapsed.as_millis());
                    break;
                }
            }
        }

        println!("\n=== Sequence Demo Complete ===\n");
    }

    /// Demonstrates custom retry policy creation
    #[tokio::test]
    async fn demo_custom_retry_policy() {
        println!("\n=== Custom Retry Policy Demo ===\n");

        // Create a custom policy for a specific use case
        let custom_policy = RetryPolicy {
            max_retries: 2,                  // Only 2 retries for quick operations
            initial_delay_ms: 500,           // Start with 500ms
            max_delay_ms: 5000,              // Cap at 5 seconds
            backoff_multiplier: 1.5,         // Gentler backoff
            retry_on_status_codes: vec![502, 503, 504], // Only retry server errors
        };

        let manager = RetryManager::new(custom_policy)
            .with_total_timeout(Duration::from_secs(30))  // 30 second total limit
            .with_jitter(false);                          // No jitter for predictability

        println!("🛠️  Custom Policy Configuration:");
        println!("  Max retries: {}", manager.get_policy().max_retries);
        println!("  Initial delay: {}ms", manager.get_policy().initial_delay_ms);
        println!("  Max delay: {}ms", manager.get_policy().max_delay_ms);
        println!("  Backoff multiplier: {}", manager.get_policy().backoff_multiplier);
        println!("  Total timeout: {}s", manager.get_max_total_duration().as_secs());
        println!("  Jitter: disabled for demo");

        // Test the custom policy
        let start_time = Instant::now();
        let server_error = ConnectorError::ExecutionFailed("HTTP 503".to_string());

        println!("\n🧪 Testing custom policy with 503 error:");
        for attempt in 0..4 {
            match manager.should_retry(&server_error, attempt, start_time) {
                RetryDecision::Retry { delay, attempt_info } => {
                    println!("  Attempt {}: Retry after {}ms", 
                            attempt_info.attempt_number, 
                            delay.as_millis());
                }
                RetryDecision::Stop { reason, .. } => {
                    println!("  Stop: {}", reason);
                    break;
                }
            }
        }

        println!("\n=== Custom Policy Demo Complete ===\n");
    }
}
