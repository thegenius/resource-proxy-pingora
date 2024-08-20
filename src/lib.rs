// Copyright 2024 Wladimir Palant
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![doc = include_str!("../README.md")]

mod compression;
mod compression_algorithm;
mod configuration;
mod file_writer;
mod handler;
pub mod metadata;
mod mime_matcher;
pub mod path;
pub mod range;
#[cfg(test)]
mod tests;
mod session_wrapper;
mod request_filter;
mod standard_response;
mod deserialize;

pub use compression_algorithm::{CompressionAlgorithm, UnsupportedCompressionAlgorithm};
pub use configuration::{StaticFilesConf, StaticFilesOpt};
pub use handler::StaticFilesHandler;
