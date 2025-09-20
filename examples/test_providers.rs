use openact::authflow::dsl::OpenactDsl;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let providers = vec![
        "github",
        "slack", 
        "google",
        "microsoft",
        "discord",
        "notion"
    ];

    println!("Testing openact provider templates...\n");

    for provider in providers {
        let template_path = format!("templates/providers/{}/oauth2.json", provider);
        
        if !Path::new(&template_path).exists() {
            println!("❌ {}: Template file not found", provider);
            continue;
        }

        match fs::read_to_string(&template_path) {
            Ok(content) => {
                match OpenactDsl::from_json(&content) {
                    Ok(dsl) => {
                        match dsl.validate() {
                            Ok(_) => {
                                println!("✅ {}: Valid template", provider);
                                println!("   - Provider: {}", dsl.provider.name);
                                println!("   - Type: {}", dsl.provider.provider_type);
                                println!("   - Flows: {:?}", dsl.list_flows());
                                
                                if let Some(display_name) = &dsl.provider.display_name {
                                    println!("   - Display Name: {}", display_name);
                                }
                                
                                if let Some(description) = &dsl.provider.description {
                                    println!("   - Description: {}", description);
                                }
                                
                                println!();
                            }
                            Err(e) => {
                                println!("❌ {}: Validation failed - {}", provider, e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("❌ {}: JSON parsing failed - {}", provider, e);
                    }
                }
            }
            Err(e) => {
                println!("❌ {}: File read failed - {}", provider, e);
            }
        }
    }

    println!("Provider template testing completed!");
    Ok(())
}
