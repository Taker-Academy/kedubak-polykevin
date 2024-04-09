use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    handler::{
        register_handler, health_checker_handler,
        connected_handler, get_post_handler,
        post_handler, get_user_post_handler,
        get_id_post_handler, login_handler,
    },
    AppState,
};

pub fn create_router(app_state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/healthchecker", get(health_checker_handler))
        .route("/auth/register", post(register_handler))
        .route("/auth/login", post(login_handler))
        .route("/user/me", get(connected_handler))
        .route("/post", post(post_handler))
        .route("/post", get(get_post_handler))
        .route("/post/me", get(get_user_post_handler))
        .route("/post/:id", get(get_id_post_handler))
        .with_state(app_state)
}

