#[cfg(feature = "server")]
use axum::Router;
#[cfg(feature = "server")]
use tokio::net::TcpListener;
#[cfg(feature = "server")]
use std::net::SocketAddr;

#[cfg(not(feature = "server"))]
fn main() {
    println!("❌ Server feature not enabled. Please recompile with --features server.");
    println!("💡 Usage: cargo run --features server");
}

#[cfg(feature = "server")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize observability systems
    openact::observability::init()?;
    
    tracing::info!("🚀 Starting openact server...");
    
    let authflow_router = openact::server::authflow::router::create_router_async().await;
    let core_router = openact::server::router::core_api_router().await;
    let app: Router = authflow_router.merge(core_router);
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    
    println!("🌐 Server running at: http://{}", addr);
    println!("📋 API documentation: http://{}/api/v1/authflow/health", addr);
    
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}
