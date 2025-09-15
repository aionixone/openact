use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::{
    extract::Request,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = Router::new()
        // Pagination: link mode
        .route(
            "/p1",
            get(|| async { axum::Json(serde_json::json!({"items":[1], "next": "/p2"})) }),
        )
        .route(
            "/p2",
            get(|| async { axum::Json(serde_json::json!({"items":[2], "next": "/p3"})) }),
        )
        .route(
            "/p3",
            get(|| async { axum::Json(serde_json::json!({"items":[3], "next": null})) }),
        )
        // NDJSON stream
        .route("/stream", get(ndjson_stream))
        // Multipart upload echo
        .route("/upload", post(upload));

    let listener = TcpListener::bind("127.0.0.1:8091").await.unwrap();
    println!(
        "mock_http listening on http://{}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app).await.unwrap();
}

async fn ndjson_stream() -> Response {
    let body = "{\"a\":1}\n{\"b\":2}\n";
    ([("content-type", "application/x-ndjson")], body).into_response()
}

async fn upload(req: Request) -> (StatusCode, axum::Json<serde_json::Value>) {
    let (parts, body) = req.into_parts();
    let ct = parts
        .headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = to_bytes(body, 4 * 1024 * 1024).await.unwrap_or_default();
    let text = String::from_utf8(bytes.to_vec()).unwrap_or_default();
    let has_fields = text.contains("name=\"file\"") && text.contains("name=\"a\"");
    let ok = has_fields || ct.contains("multipart/form-data");
    let preview = &text[..text.len().min(128)];
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "ok": ok,
            "content_type": ct,
            "len": text.len(),
            "preview": preview
        })),
    )
}
