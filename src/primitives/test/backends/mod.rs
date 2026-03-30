// SPDX-License-Identifier: MIT
//! Backend implementations for different test frameworks.

mod bats;
mod go;
mod jest;
mod make;
mod npm;
mod pytest;
mod rust;
mod vitest;

pub use bats::BatsBackend;
pub use go::GoBackend;
pub use jest::JestBackend;
pub use make::MakeBackend;
pub use npm::NpmBackend;
pub use pytest::PytestBackend;
pub use rust::CargoBackend;
pub use vitest::VitestBackend;
