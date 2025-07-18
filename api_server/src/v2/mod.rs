//! API v2 Module - Enhanced P&L Analysis for Copy Trading
//! 
//! This module provides enhanced API endpoints that expose the full capabilities
//! of the new P&L engine without legacy conversion, enabling detailed copy trading analysis.

pub mod routes;
pub mod handlers;
pub mod types;

pub use routes::create_v2_routes;