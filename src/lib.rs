#![doc = include_str!("../README.md")]
#![expect(
    clippy::module_name_repetitions,
    reason = "endpoint types are named after the Jenkins operation they perform (GetJob, GetBuild, ListQueue), which reads better at the call site than stripping the module name would"
)]

pub mod api;
pub mod config;
pub mod error;
#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod live_tests;
pub mod types;

pub use api::{Endpoint, Jenkins, JenkinsAsync, ParamValue, QueryParams, ReturnsJsonResponse};
pub use config::{JenkinsAuth, JenkinsInstance};
pub use error::Error;
pub use types::{
    Build, BuildAction, BuildNumber, BuildResult, BuildsWrapper, Cause, Crumb, DurationMillis, Job,
    JobName, JobsWrapper, QueueItem, QueueItemId, QueueItemsWrapper, Revision, Scm, Timestamp,
    View, ViewName,
};
