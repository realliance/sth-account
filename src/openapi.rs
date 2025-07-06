use axum::{Extension, Router, response::Json, routing::get};
use std::sync::Arc;

async fn openapi_json(
    Extension(spec): Extension<Arc<utoipa::openapi::OpenApi>>,
) -> Json<Arc<utoipa::openapi::OpenApi>> {
    Json(spec)
}

pub fn add_docs_routes(router: Router, openapi_spec: utoipa::openapi::OpenApi) -> Router {
    router
        .route("/openapi.json", get(openapi_json))
        .layer(Extension(Arc::new(openapi_spec)))
}
