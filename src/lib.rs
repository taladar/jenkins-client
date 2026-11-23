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
pub mod job_config;
#[cfg(test)]
mod live_tests;
pub mod types;

pub use api::{Endpoint, Jenkins, JenkinsAsync, ParamValue, QueryParams, ReturnsJsonResponse};
pub use config::{JenkinsAuth, JenkinsInstance};
pub use error::Error;
pub use job_config::{JobConfig, JobTrigger};
pub use types::{
    Build, BuildAction, BuildNumber, BuildResult, BuildsWrapper, Cause, Computer, ConsoleChunk,
    ConsoleFilter, Crumb, DurationMillis, Executor, ExecutorRun, Job, JobName, JobsWrapper,
    QueueItem, QueueItemId, QueueItemsWrapper, Revision, RunningBuild, Scm, Timestamp, View,
    ViewName,
};
