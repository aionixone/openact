#[cfg(feature = "server")]
use axum::Router;
#[cfg(feature = "server")]
use tokio::net::TcpListener;
#[cfg(feature = "server")]
use std::net::SocketAddr;

#[cfg(not(feature = "server"))]
fn main() {
    println!("❌ 服务器功能未启用。请使用 --features server 重新编译。");
    println!("💡 使用方法: cargo run --features server");
}

#[cfg(feature = "server")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 启动 openact 服务器...");
    
    let authflow_router = openact::server::authflow::router::create_router_async().await;
    let core_router = openact::server::router::core_api_router();
    let app: Router = authflow_router.merge(core_router);
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    
    println!("🌐 服务器运行在: http://{}", addr);
    println!("📋 API 文档: http://{}/api/v1/authflow/health", addr);
    
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}
