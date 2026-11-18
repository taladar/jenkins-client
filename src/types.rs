//! The data types the Jenkins REST API returns, plus the newtypes used to
//! refer to jobs, builds and queue items.

use serde::{Deserialize, Serialize};

/// the name of a Jenkins job
///
/// Job names are free-form and in practice wildly inconsistent — the same
/// Jenkins can host both `saltation___foo___Debian_Rust_Package___bookworm___amd64`
/// and `saltation - foo - Check Only Rust Package - bookworm`. In particular
/// they may contain spaces, so they must be percent-encoded before being used
/// as a URL path segment; [`crate::api::Jenkins`] does that for you.
#[derive(
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_more::Display,
    derive_more::FromStr,
)]
pub struct JobName(pub String);

impl From<&str> for JobName {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for JobName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// the number of a build within a job
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_more::Display,
    derive_more::FromStr,
)]
pub struct BuildNumber(pub u64);

/// the id of an item in the Jenkins build queue
///
/// A queue item is what a build trigger produces before an executor picks it
/// up; once it starts it turns into a build with a [BuildNumber].
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_more::Display,
    derive_more::FromStr,
)]
pub struct QueueItemId(pub u64);

/// the name of a Jenkins view
#[derive(
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_more::Display,
    derive_more::FromStr,
)]
pub struct ViewName(pub String);

impl From<&str> for ViewName {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for ViewName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// a git revision (SHA1) as Jenkins reports it in build data
#[derive(
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_more::Display,
    derive_more::FromStr,
)]
pub struct Revision(pub String);

impl From<&str> for Revision {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for Revision {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// a point in time as Jenkins reports it, in milliseconds since the Unix epoch
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// convert this timestamp into a [`time::OffsetDateTime`]
    ///
    /// Returns `None` if the value is so far from the epoch that it does not
    /// fit the supported range, which should not happen for timestamps a real
    /// Jenkins produces.
    #[must_use]
    pub fn to_offset_date_time(self) -> Option<time::OffsetDateTime> {
        let nanos = i128::from(self.0).checked_mul(1_000_000)?;
        time::OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()
    }
}

/// a duration as Jenkins reports it, in milliseconds
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DurationMillis(pub i64);

impl DurationMillis {
    /// convert this duration into a [`std::time::Duration`]
    ///
    /// Returns `None` for negative values, which Jenkins uses for builds that
    /// have not finished yet.
    #[must_use]
    pub fn to_duration(self) -> Option<std::time::Duration> {
        u64::try_from(self.0)
            .ok()
            .map(std::time::Duration::from_millis)
    }
}

/// the outcome of a finished Jenkins build
///
/// This deliberately does *not* have a variant for a build that is still
/// running: Jenkins reports `"result": null` while a build is in progress, so
/// [`Build::result`] is an `Option<BuildResult>` and [`Build::building`] tells
/// you which of the two reasons a `None` has.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildResult {
    /// the build succeeded
    #[serde(rename = "SUCCESS")]
    Success,
    /// the build itself succeeded but something it reported (usually tests)
    /// did not
    #[serde(rename = "UNSTABLE")]
    Unstable,
    /// the build failed
    #[serde(rename = "FAILURE")]
    Failure,
    /// the build was not run, e.g. because an upstream build failed
    #[serde(rename = "NOT_BUILT")]
    NotBuilt,
    /// the build was aborted, by a user or by a timeout
    #[serde(rename = "ABORTED")]
    Aborted,
}

impl BuildResult {
    /// whether this result should be considered a success
    ///
    /// Only [`BuildResult::Success`] qualifies. `UNSTABLE` deliberately does
    /// not: a caller gating a release on test results wants to know about it,
    /// and a caller that does not care can match on the variant directly.
    #[must_use]
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Success)
    }

    /// parse the name Jenkins uses for a result, e.g. `SUCCESS`
    ///
    /// The JSON API deserializes results through serde; this is for the
    /// places that carry the name as plain text, such as the threshold of an
    /// upstream trigger in a job's `config.xml`.
    #[must_use]
    pub fn from_jenkins_name(name: &str) -> Option<Self> {
        match name {
            "SUCCESS" => Some(Self::Success),
            "UNSTABLE" => Some(Self::Unstable),
            "FAILURE" => Some(Self::Failure),
            "NOT_BUILT" => Some(Self::NotBuilt),
            "ABORTED" => Some(Self::Aborted),
            _ => None,
        }
    }
}

impl std::fmt::Display for BuildResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match *self {
            Self::Success => "SUCCESS",
            Self::Unstable => "UNSTABLE",
            Self::Failure => "FAILURE",
            Self::NotBuilt => "NOT_BUILT",
            Self::Aborted => "ABORTED",
        };
        f.write_str(s)
    }
}

/// one entry of a job's git SCM `userRemoteConfigs`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRemoteConfig {
    /// the git remote URL the job clones from
    #[serde(default)]
    pub url: Option<String>,
    /// the name of the remote, usually `origin`
    #[serde(default)]
    pub name: Option<String>,
    /// the id of the Jenkins credential used to authenticate against the remote
    #[serde(default)]
    pub credentials_id: Option<String>,
    /// the refspec configured for the remote, if any
    #[serde(default)]
    pub refspec: Option<String>,
}

/// one entry of a job's git SCM branch specification
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BranchSpec {
    /// the branch name or pattern the job builds, e.g. `master` or
    /// `build_bookworm`
    #[serde(default)]
    pub name: Option<String>,
}

/// the SCM configuration of a job
///
/// Jobs that are triggered by an upstream job rather than by an SCM change
/// have `hudson.scm.NullSCM` here, which means both `branches` and
/// `user_remote_configs` are empty. That is how the weekly cron jobs and the
/// downstream repo-publish jobs are distinguished from jobs actually bound to
/// a git repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scm {
    /// the Jenkins class name of the SCM implementation, e.g.
    /// `hudson.plugins.git.GitSCM` or `hudson.scm.NullSCM`
    #[serde(rename = "_class", default)]
    pub class: Option<String>,
    /// the branches this job builds
    #[serde(default)]
    pub branches: Vec<BranchSpec>,
    /// the git remotes this job clones from
    #[serde(default)]
    pub user_remote_configs: Vec<UserRemoteConfig>,
}

/// a Jenkins job
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    /// the Jenkins class name of the job type, e.g.
    /// `hudson.model.FreeStyleProject`
    #[serde(rename = "_class", default)]
    pub class: Option<String>,
    /// the job name, which is also its URL path segment
    #[serde(default)]
    pub name: Option<JobName>,
    /// the absolute URL of the job in the Jenkins web UI
    #[serde(default)]
    pub url: Option<String>,
    /// the job's status ball colour, e.g. `blue`, `red` or `disabled`
    #[serde(default)]
    pub color: Option<String>,
    /// whether the job can currently be built
    #[serde(default)]
    pub buildable: Option<bool>,
    /// whether the job is currently disabled
    #[serde(default)]
    pub disabled: Option<bool>,
    /// whether the job currently has an item waiting in the build queue
    #[serde(default)]
    pub in_queue: Option<bool>,
    /// the number the next build of this job will get
    #[serde(default)]
    pub next_build_number: Option<BuildNumber>,
    /// the SCM configuration of the job
    #[serde(default)]
    pub scm: Option<Scm>,
    /// the builds of this job, when requested
    #[serde(default)]
    pub builds: Vec<Build>,
    /// the jobs this job triggers when it succeeds
    #[serde(default)]
    pub downstream_projects: Vec<Self>,
    /// the jobs that trigger this job
    #[serde(default)]
    pub upstream_projects: Vec<Self>,
}

/// the git revision a build actually checked out
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BuiltRevision {
    /// the commit that was built
    #[serde(rename = "SHA1")]
    pub sha1: Revision,
    /// the branches that pointed at that commit
    #[serde(default)]
    pub branch: Vec<BuiltBranch>,
}

/// one branch entry of a [BuiltRevision]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BuiltBranch {
    /// the commit the branch pointed at
    #[serde(rename = "SHA1")]
    pub sha1: Revision,
    /// the branch name, including the remote prefix, e.g.
    /// `origin/build_bookworm`
    #[serde(default)]
    pub name: Option<String>,
}

/// why a build was started
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cause {
    /// the Jenkins class name of the cause, e.g.
    /// `hudson.model.Cause$UpstreamCause`
    #[serde(rename = "_class", default)]
    pub class: Option<String>,
    /// a human-readable description of the cause
    #[serde(default)]
    pub short_description: Option<String>,
    /// for an upstream cause, the job that triggered this build
    #[serde(default)]
    pub upstream_project: Option<JobName>,
    /// for an upstream cause, the build of that job which triggered this build
    #[serde(default)]
    pub upstream_build: Option<BuildNumber>,
}

/// one entry of a build's `actions` array
///
/// The Jenkins `actions` array is heterogeneous: each plugin contributes its
/// own object, and when a `tree=` filter is used the entries that do not match
/// come back as empty objects. Rather than model every plugin's action type,
/// this flattens the handful of fields we care about into one struct where
/// everything is optional, so an empty `{}` deserializes into an all-default
/// value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildAction {
    /// the Jenkins class name of the action, when requested
    #[serde(rename = "_class", default)]
    pub class: Option<String>,
    /// the revision this build checked out, from the git plugin's `BuildData`
    #[serde(default)]
    pub last_built_revision: Option<BuiltRevision>,
    /// the git remote URLs this build cloned from
    #[serde(default)]
    pub remote_urls: Vec<String>,
    /// why this build was started
    #[serde(default)]
    pub causes: Vec<Cause>,
}

/// a single build of a Jenkins job
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    /// the Jenkins class name of the build type
    #[serde(rename = "_class", default)]
    pub class: Option<String>,
    /// the build number within its job
    #[serde(default)]
    pub number: Option<BuildNumber>,
    /// the outcome, or `None` while the build is still running
    #[serde(default)]
    pub result: Option<BuildResult>,
    /// whether the build is currently running
    #[serde(default)]
    pub building: Option<bool>,
    /// when the build started
    #[serde(default)]
    pub timestamp: Option<Timestamp>,
    /// how long the build took, or a negative value while it is running
    #[serde(default)]
    pub duration: Option<DurationMillis>,
    /// the absolute URL of the build in the Jenkins web UI
    #[serde(default)]
    pub url: Option<String>,
    /// the build's actions, which carry the git revision and the causes
    #[serde(default)]
    pub actions: Vec<BuildAction>,
}

impl Build {
    /// the git revision this build checked out, if it recorded one
    ///
    /// Downstream jobs triggered by an upstream job rather than by an SCM
    /// change have no git data at all and return `None` here; correlate those
    /// via [`Build::upstream_cause`] instead.
    #[must_use]
    pub fn built_revision(&self) -> Option<&Revision> {
        self.actions
            .iter()
            .find_map(|action| action.last_built_revision.as_ref())
            .map(|revision| &revision.sha1)
    }

    /// the upstream job and build that triggered this build, if any
    #[must_use]
    pub fn upstream_cause(&self) -> Option<(&JobName, BuildNumber)> {
        self.actions
            .iter()
            .flat_map(|action| action.causes.iter())
            .find_map(|cause| {
                let project = cause.upstream_project.as_ref()?;
                let build = cause.upstream_build?;
                Some((project, build))
            })
    }

    /// whether this build has reached a terminal state
    ///
    /// A build is terminal once Jenkins reports a result and is no longer
    /// building. Both are checked because a build that has just been created
    /// can briefly have neither set.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.result.is_some() && self.building != Some(true)
    }
}

/// the build a queue item turned into once an executor picked it up
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueExecutable {
    /// the number the queued build got
    #[serde(default)]
    pub number: Option<BuildNumber>,
    /// the absolute URL of the build in the Jenkins web UI
    #[serde(default)]
    pub url: Option<String>,
}

/// the job a queue item is waiting to build
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct QueueTask {
    /// the job name
    #[serde(default)]
    pub name: Option<JobName>,
    /// the absolute URL of the job in the Jenkins web UI
    #[serde(default)]
    pub url: Option<String>,
}

/// an item waiting in the Jenkins build queue
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    /// the Jenkins class name of the queue item
    #[serde(rename = "_class", default)]
    pub class: Option<String>,
    /// the queue item id
    #[serde(default)]
    pub id: Option<QueueItemId>,
    /// a human-readable explanation of why the item is still queued
    #[serde(default)]
    pub why: Option<String>,
    /// whether the item is blocked by something else
    #[serde(default)]
    pub blocked: Option<bool>,
    /// whether the item could run if an executor were free
    #[serde(default)]
    pub buildable: Option<bool>,
    /// whether Jenkins considers the item stuck
    #[serde(default)]
    pub stuck: Option<bool>,
    /// whether the item was cancelled before it started
    #[serde(default)]
    pub cancelled: Option<bool>,
    /// the build the item turned into, once it started
    #[serde(default)]
    pub executable: Option<QueueExecutable>,
    /// the job the item is waiting to build
    #[serde(default)]
    pub task: Option<QueueTask>,
}

/// the CSRF crumb Jenkins requires for state-changing requests
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    /// the crumb value
    pub crumb: String,
    /// the name of the header the crumb has to be sent in, which is
    /// `Jenkins-Crumb` on current versions
    pub crumb_request_field: String,
}

/// the identity Jenkins believes the current request belongs to
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WhoAmI {
    /// whether the request was authenticated at all
    #[serde(default)]
    pub authenticated: Option<bool>,
    /// the username Jenkins resolved the credentials to
    #[serde(default)]
    pub name: Option<String>,
}

/// a Jenkins view
///
/// With the nested-view plugin a view can contain other views as well as jobs,
/// so this is a tree rather than a flat list. A plain `ListView` or
/// `SectionedView` has jobs and no children; a `NestedView` is the other way
/// round.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// the Jenkins class name of the view type, e.g.
    /// `hudson.plugins.nested_view.NestedView` or `hudson.model.ListView`
    #[serde(rename = "_class", default)]
    pub class: Option<String>,
    /// the view name
    #[serde(default)]
    pub name: Option<ViewName>,
    /// the absolute URL of the view in the Jenkins web UI
    #[serde(default)]
    pub url: Option<String>,
    /// the jobs directly in this view
    #[serde(default)]
    pub jobs: Vec<Job>,
    /// the views nested inside this one
    #[serde(default)]
    pub views: Vec<Self>,
}

impl View {
    /// the jobs in this view and, recursively, in every view nested inside it
    ///
    /// This is what "the jobs of a view" means to a person looking at a nested
    /// view in the web UI, where the child views are part of what they are
    /// looking at.
    #[must_use]
    pub fn all_jobs(&self) -> Vec<&Job> {
        let mut jobs: Vec<&Job> = self.jobs.iter().collect();
        for nested in &self.views {
            jobs.extend(nested.all_jobs());
        }
        jobs
    }

    /// find a view by its path from this view downwards, e.g.
    /// `["Overview", "Beam"]` starting at the root
    #[must_use]
    pub fn find_path(&self, path: &[ViewName]) -> Option<&Self> {
        let Some((first, rest)) = path.split_first() else {
            return Some(self);
        };
        let child = self
            .views
            .iter()
            .find(|view| view.name.as_ref() == Some(first))?;
        child.find_path(rest)
    }

    /// every view at or below this one whose name matches, paired with the
    /// path that reaches it
    ///
    /// Used to resolve a bare view name typed by a human, which is ambiguous
    /// when the same name appears in more than one branch of the tree.
    #[must_use]
    pub fn find_by_name(&self, wanted: &ViewName) -> Vec<(Vec<ViewName>, &Self)> {
        let mut found = Vec::new();
        self.collect_by_name(wanted, &mut Vec::new(), &mut found);
        found
    }

    /// recursive worker for [`Self::find_by_name`]
    fn collect_by_name<'a>(
        &'a self,
        wanted: &ViewName,
        prefix: &mut Vec<ViewName>,
        found: &mut Vec<(Vec<ViewName>, &'a Self)>,
    ) {
        for child in &self.views {
            let Some(name) = child.name.as_ref() else {
                continue;
            };
            prefix.push(name.clone());
            if name == wanted {
                found.push((prefix.clone(), child));
            }
            child.collect_by_name(wanted, prefix, found);
            prefix.pop();
        }
    }
}

/// a wrapper for API responses that return a list of jobs
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct JobsWrapper<T> {
    /// the jobs
    #[serde(default = "Vec::new")]
    pub jobs: Vec<T>,
}

/// a wrapper for API responses that return a list of builds
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct BuildsWrapper<T> {
    /// the builds
    #[serde(default = "Vec::new")]
    pub builds: Vec<T>,
}

/// a wrapper for API responses that return a list of views
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ViewsWrapper<T> {
    /// the views
    #[serde(default = "Vec::new")]
    pub views: Vec<T>,
}

/// a wrapper for API responses that return a list of queue items
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct QueueItemsWrapper<T> {
    /// the queue items
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::error::Error;

    #[test]
    fn test_deserialize_build_result() -> Result<(), Box<dyn Error>> {
        assert_eq!(
            serde_json::from_str::<BuildResult>("\"SUCCESS\"")?,
            BuildResult::Success
        );
        assert_eq!(
            serde_json::from_str::<BuildResult>("\"NOT_BUILT\"")?,
            BuildResult::NotBuilt
        );
        assert_eq!(serde_json::from_str::<Option<BuildResult>>("null")?, None);
        Ok(())
    }

    #[test]
    fn test_empty_actions_entries_deserialize() -> Result<(), Box<dyn Error>> {
        // the tree= filter makes Jenkins return {} for every action that does
        // not carry any of the requested fields
        let build: Build = serde_json::from_str(
            r#"{"number": 23, "result": "SUCCESS", "building": false,
                "actions": [{}, {}, {"_class": "hudson.model.CauseAction"}]}"#,
        )?;
        assert_eq!(build.number, Some(BuildNumber(23)));
        assert_eq!(build.actions.len(), 3);
        assert_eq!(build.built_revision(), None);
        assert!(build.is_finished());
        Ok(())
    }

    fn view(name: &str, jobs: &[&str], children: Vec<View>) -> View {
        View {
            class: None,
            name: Some(ViewName(name.to_owned())),
            url: None,
            jobs: jobs
                .iter()
                .map(|j| Job {
                    name: Some(JobName((*j).to_owned())),
                    ..Job::default()
                })
                .collect(),
            views: children,
        }
    }

    fn sample_tree() -> View {
        // the real shape: a NestedView root, some nested branches, some leaves
        view(
            "root",
            &[],
            vec![
                view("Beam", &["beam-a", "beam-b"], Vec::new()),
                view(
                    "Beltz",
                    &[],
                    vec![view("Website", &["beltz-web"], Vec::new())],
                ),
                view(
                    "Other",
                    &[],
                    vec![view("Website", &["other-web"], Vec::new())],
                ),
            ],
        )
    }

    #[test]
    fn test_all_jobs_includes_nested_views() {
        let tree = sample_tree();
        let names: Vec<&str> = tree
            .all_jobs()
            .iter()
            .filter_map(|j| j.name.as_ref().map(|n| n.0.as_str()))
            .collect();
        assert_eq!(names, vec!["beam-a", "beam-b", "beltz-web", "other-web"]);
    }

    #[test]
    fn test_find_path_walks_the_tree() {
        let tree = sample_tree();
        let found = tree.find_path(&[ViewName("Beltz".to_owned()), ViewName("Website".to_owned())]);
        assert_eq!(
            found.and_then(|v| v.name.clone()),
            Some(ViewName("Website".to_owned()))
        );
        assert_eq!(
            found.map(|v| v.all_jobs().len()),
            Some(1),
            "the Beltz Website view has one job, not both Website views' jobs"
        );
        assert!(tree.find_path(&[ViewName("Nope".to_owned())]).is_none());
    }

    #[test]
    fn test_find_by_name_reports_every_match_with_its_path() {
        // "Website" exists under two different parents, which is exactly the
        // ambiguity a bare name has to surface rather than guess at
        let tree = sample_tree();
        let matches = tree.find_by_name(&ViewName("Website".to_owned()));
        let paths: Vec<String> = matches
            .iter()
            .map(|(path, _)| {
                path.iter()
                    .map(|n| n.0.as_str())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect();
        assert_eq!(paths, vec!["Beltz/Website", "Other/Website"]);
    }

    #[test]
    fn test_find_by_name_unique_match() {
        let tree = sample_tree();
        let matches = tree.find_by_name(&ViewName("Beam".to_owned()));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches.first().map(|(p, _)| p.len()), Some(1));
    }

    #[test]
    fn test_timestamp_and_duration_conversion() -> Result<(), Box<dyn Error>> {
        let ts = Timestamp(1_756_204_367_000);
        let odt = ts.to_offset_date_time().ok_or("timestamp out of range")?;
        assert_eq!(odt.unix_timestamp(), 1_756_204_367);
        assert_eq!(
            DurationMillis(161_496).to_duration(),
            Some(std::time::Duration::from_millis(161_496))
        );
        // Jenkins uses a negative duration for a build that is still running
        assert_eq!(DurationMillis(-1).to_duration(), None);
        Ok(())
    }
}
