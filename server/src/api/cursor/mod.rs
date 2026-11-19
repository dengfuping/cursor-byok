//! Wires the Cursor-facing API routes.

pub mod bidi;
mod handlers;
mod inference_sse;
pub mod proxy;
mod run_sse;

pub use handlers::router;
