// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Pluggable Policies
//!
//! This module provides pluggable policy interfaces for:
//! - Routing algorithms
//! - Scheduling/queueing disciplines
//! - QoS management

pub mod qos;
pub mod routing;
pub mod scheduling;

pub use qos::{QoSPolicy, SimpleQoSPolicy};
pub use routing::{RoutingPolicy, ShortestPathRouting};
pub use scheduling::{SchedulingPolicy, FifoScheduling, PriorityScheduling};
