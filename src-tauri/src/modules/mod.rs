//! 扩展框架聚合：ModuleHost / MessageBus / 权限门控 / 清单契约 / 运行时。
//!
//! 该模块作为扩展框架的统一出口，`pub use` 便于外部以 `crate::modules::Xxx` 引用。

#![allow(unused_imports)]

pub mod bus;
pub mod capabilities;
pub mod host;
pub mod instance;
pub mod manifest;
pub mod sidecar;
pub mod wasm;

pub use bus::MessageBus;
pub use capabilities::{Capability, CapabilityGate};
pub use host::{ContributionEntry, ModuleHost, ModuleRecord};
pub use instance::{ModuleInstance, ModuleMessage, ModuleState};
pub use manifest::{Contribution, Lifecycle, ModuleEntry, ModuleManifest, ModuleRuntime};
