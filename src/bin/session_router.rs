#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use axum::extract::{Path, State};
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use rts::session_router::{
        HostedSessionRegistration, HostedSessionSummary, SharedSessionDirectory,
        make_shared_directory, public_ws_path, replay_instruction,
    };
    use std::net::SocketAddr;
    use tower_http::services::ServeDir;
    use tower_http::trace::TraceLayer;

    #[derive(Clone)]
    struct AppState {
        sessions: SharedSessionDirectory,
    }

    #[tokio::main]
    pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState {
            sessions: make_shared_directory(),
        };

        let dist_dir = std::env::var("DIST_DIR").unwrap_or_else(|_| "dist".to_string());
        let app = build_router(state, dist_dir);

        let port = std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(8080);
        let addr = SocketAddr::from(([0, 0, 0, 0], port));

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }

    fn build_router(state: AppState, dist_dir: String) -> Router {
        Router::new()
            .route("/healthz", get(healthz))
            .route("/api/sessions", post(register_session))
            .route("/api/sessions/{code}", get(get_session))
            .route("/session/{code}/ws", get(replay_session_ws))
            .fallback_service(ServeDir::new(dist_dir).append_index_html_on_directories(true))
            .layer(TraceLayer::new_for_http())
            .with_state(state)
    }

    async fn healthz() -> &'static str {
        "ok"
    }

    async fn register_session(
        State(state): State<AppState>,
        Json(payload): Json<HostedSessionRegistration>,
    ) -> Result<Json<HostedSessionSummary>, (StatusCode, String)> {
        let mut sessions = state
            .sessions
            .write()
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session directory poisoned".to_string(),
                )
            })?;
        let record = sessions
            .register(payload)
            .map_err(|err| (StatusCode::BAD_REQUEST, err))?;
        let ws_path = public_ws_path(&record.code).map_err(|err| (StatusCode::BAD_REQUEST, err))?;
        Ok(Json(HostedSessionSummary {
            code: record.code,
            ws_path,
        }))
    }

    async fn get_session(
        State(state): State<AppState>,
        Path(code): Path<String>,
    ) -> Result<Json<HostedSessionSummary>, (StatusCode, String)> {
        let sessions = state
            .sessions
            .read()
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session directory poisoned".to_string(),
                )
            })?;
        let record = sessions
            .get(&code)
            .ok_or_else(|| (StatusCode::NOT_FOUND, "session not found".to_string()))?;
        let ws_path = public_ws_path(&record.code).map_err(|err| (StatusCode::BAD_REQUEST, err))?;
        Ok(Json(HostedSessionSummary {
            code: record.code,
            ws_path,
        }))
    }

    async fn replay_session_ws(
        State(state): State<AppState>,
        Path(code): Path<String>,
    ) -> Result<Response, (StatusCode, String)> {
        let sessions = state
            .sessions
            .read()
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session directory poisoned".to_string(),
                )
            })?;
        let record = sessions
            .get(&code)
            .ok_or_else(|| (StatusCode::NOT_FOUND, "session not found".to_string()))?;

        let replay = replay_instruction(&record);
        let replay_json = serde_json::to_string(&replay)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.fly.replay+json"),
        );
        headers.insert("fly-replay", HeaderValue::from_static("true"));

        Ok((StatusCode::OK, headers, replay_json).into_response())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use axum::body::{Body, to_bytes};
        use axum::http::Request;
        use rts::session_router::SessionDirectory;
        use tower::ServiceExt;

        fn test_app() -> Router {
            let state = AppState {
                sessions: std::sync::Arc::new(std::sync::RwLock::new(SessionDirectory::default())),
            };
            build_router(state, "dist".to_string())
        }

        #[tokio::test]
        async fn register_session_returns_public_ws_path() {
            let app = test_app();
            let request = Request::builder()
                .method("POST")
                .uri("/api/sessions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&HostedSessionRegistration {
                        code: "ABCD12".to_string(),
                        app: Some("rts-game".to_string()),
                        machine_id: "machine-1".to_string(),
                        region: Some("ams".to_string()),
                        target_path: "/ws".to_string(),
                    })
                    .unwrap(),
                ))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let summary: HostedSessionSummary = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                summary,
                HostedSessionSummary {
                    code: "ABCD12".to_string(),
                    ws_path: "/session/ABCD12/ws".to_string(),
                }
            );
        }

        #[tokio::test]
        async fn replay_session_ws_returns_fly_replay_payload() {
            let state = AppState {
                sessions: std::sync::Arc::new(std::sync::RwLock::new(SessionDirectory::default())),
            };
            state
                .sessions
                .write()
                .unwrap()
                .register(HostedSessionRegistration {
                    code: "ABCD12".to_string(),
                    app: Some("rts-game".to_string()),
                    machine_id: "machine-1".to_string(),
                    region: Some("ams".to_string()),
                    target_path: "/ws".to_string(),
                })
                .unwrap();
            let app = build_router(state, "dist".to_string());

            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/session/ABCD12/ws")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers().get("content-type").unwrap(),
                "application/vnd.fly.replay+json"
            );
            assert_eq!(response.headers().get("fly-replay").unwrap(), "true");

            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["instance"], "machine-1");
            assert_eq!(payload["app"], "rts-game");
            assert_eq!(payload["region"], "ams");
            assert_eq!(payload["transform"]["path"], "/ws");
        }

        #[tokio::test]
        async fn get_missing_session_returns_not_found() {
            let app = test_app();
            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/sessions/NOPE")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::main()
}
