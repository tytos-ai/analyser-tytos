// Middleware functions for the API server
// Currently empty - middleware is handled by axum's built-in middleware and tower-http
// This file is kept for future middleware implementations if needed

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use tracing::debug;

// Note: All middleware is currently handled by:
// - tower-http::cors::CorsLayer for CORS
// - tower-http::trace::TraceLayer for request logging
// - Custom error handling in handlers