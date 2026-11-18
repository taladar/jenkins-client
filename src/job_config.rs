//! A read-only, typed view of a job's `config.xml`.
//!
//! The JSON API leaves out some of a job's configuration altogether, and its
//! triggers are the gap that matters most: whether a job is started by a push,
//! by a timer, by an upstream job or only by hand is visible nowhere but in
//! `config.xml`. This module turns the parts of that document worth having
//! into plain Rust types — callers never see an XML tree — and deliberately
//! nothing more: the document is shaped by whichever plugins a job uses, so it
//! cannot be modelled completely, and this crate does not write job
//! configuration anyway.

use crate::types::{BuildResult, JobName};

/// the parts of a job's `config.xml` this crate understands
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobConfig {
    /// the root element of the document, which names the kind of job, e.g.
    /// `project` for a freestyle job or `flow-definition` for a pipeline
    pub kind: String,
    /// whether the job is disabled
    pub disabled: bool,
    /// the job's build triggers, in document order
    pub triggers: Vec<JobTrigger>,
}

/// a build trigger configured on a job
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum JobTrigger {
    /// the git plugin's "Poll SCM" trigger (`hudson.triggers.SCMTrigger`)
    ///
    /// Besides polling on its schedule, this is the trigger a forge's push
    /// notification (`notifyCommit`) acts on: Jenkins answers it by scheduling
    /// a poll of every job with this trigger that builds the notified
    /// repository.
    ScmPoll {
        /// the cron-style polling schedule, `None` when the job only reacts to
        /// push notifications
        schedule: Option<String>,
        /// whether push notifications are ignored, leaving only the schedule
        ignore_post_commit_hooks: bool,
    },
    /// a cron-style schedule (`hudson.triggers.TimerTrigger`)
    Timer {
        /// the cron-style schedule, `None` if the trigger has none, in which
        /// case it never fires
        schedule: Option<String>,
    },
    /// "build after other projects are built"
    /// (`jenkins.triggers.ReverseBuildTrigger`)
    Upstream {
        /// the jobs whose builds trigger this one
        upstream_projects: Vec<JobName>,
        /// the worst result of an upstream build that still triggers this job
        ///
        /// `None` when the threshold is missing or names a result this crate
        /// does not know.
        threshold: Option<BuildResult>,
    },
    /// the GitLab plugin's webhook trigger
    /// (`com.dabsquared.gitlabjenkins.GitLabPushTrigger`)
    ///
    /// Despite its class name it covers every GitLab event the plugin
    /// handles, so whether a push starts the job depends on `on_push`.
    GitLab {
        /// whether a push event starts a build
        on_push: bool,
        /// whether opening or updating a merge request starts a build
        on_merge_request: bool,
        /// whether a merge request comment matching the job's note regex
        /// starts a build
        on_note: bool,
    },
    /// the GitHub plugin's push trigger
    /// (`com.cloudbees.jenkins.GitHubPushTrigger`)
    GitHubPush,
    /// any other trigger, identified by its class name
    Other {
        /// the trigger's class name, which is its element name in the XML
        class: String,
    },
}

/// the class name of the SCM polling trigger
const SCM_TRIGGER: &str = "hudson.triggers.SCMTrigger";
/// the class name of the timer trigger
const TIMER_TRIGGER: &str = "hudson.triggers.TimerTrigger";
/// the class name of the upstream trigger
const REVERSE_BUILD_TRIGGER: &str = "jenkins.triggers.ReverseBuildTrigger";
/// the class name of the GitLab plugin's trigger
const GITLAB_TRIGGER: &str = "com.dabsquared.gitlabjenkins.GitLabPushTrigger";
/// the class name of the GitHub plugin's push trigger
const GITHUB_PUSH_TRIGGER: &str = "com.cloudbees.jenkins.GitHubPushTrigger";
/// the job property pipeline jobs keep their triggers in
const PIPELINE_TRIGGERS_PROPERTY: &str =
    "org.jenkinsci.plugins.workflow.job.properties.PipelineTriggersJobProperty";

impl JobTrigger {
    /// whether a push to the job's repository starts a build through this
    /// trigger
    ///
    /// A [`JobTrigger::ScmPoll`] that ignores push notifications still counts:
    /// its next poll picks the push up, just later.
    #[must_use]
    pub const fn starts_on_push(&self) -> bool {
        match self {
            Self::ScmPoll { .. } | Self::GitHubPush => true,
            Self::GitLab { on_push, .. } => *on_push,
            Self::Timer { .. } | Self::Upstream { .. } | Self::Other { .. } => false,
        }
    }

    /// the cron-style schedule this trigger fires on by itself, if any
    ///
    /// That is the timer's schedule, or the polling schedule of an SCM poll.
    #[must_use]
    pub fn schedule(&self) -> Option<&str> {
        match self {
            Self::ScmPoll { schedule, .. } | Self::Timer { schedule } => schedule.as_deref(),
            Self::Upstream { .. } | Self::GitLab { .. } | Self::GitHubPush | Self::Other { .. } => {
                None
            }
        }
    }

    /// the Jenkins class name of the trigger, as it appears in `config.xml`
    #[must_use]
    pub fn class_name(&self) -> &str {
        match self {
            Self::ScmPoll { .. } => SCM_TRIGGER,
            Self::Timer { .. } => TIMER_TRIGGER,
            Self::Upstream { .. } => REVERSE_BUILD_TRIGGER,
            Self::GitLab { .. } => GITLAB_TRIGGER,
            Self::GitHubPush => GITHUB_PUSH_TRIGGER,
            Self::Other { class } => class,
        }
    }

    /// parse one child element of a `<triggers>` element
    fn from_node(node: roxmltree::Node<'_, '_>) -> Self {
        let class = node.tag_name().name();
        match class {
            SCM_TRIGGER => Self::ScmPoll {
                schedule: child_nonempty_text(node, "spec"),
                ignore_post_commit_hooks: child_bool(node, "ignorePostCommitHooks"),
            },
            TIMER_TRIGGER => Self::Timer {
                schedule: child_nonempty_text(node, "spec"),
            },
            REVERSE_BUILD_TRIGGER => Self::Upstream {
                upstream_projects: child_text(node, "upstreamProjects")
                    .unwrap_or_default()
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(JobName::from)
                    .collect(),
                threshold: child(node, "threshold")
                    .and_then(|threshold| child_text(threshold, "name"))
                    .and_then(|name| BuildResult::from_jenkins_name(name.trim())),
            },
            GITLAB_TRIGGER => Self::GitLab {
                on_push: child_bool(node, "triggerOnPush"),
                on_merge_request: child_bool(node, "triggerOnMergeRequest"),
                on_note: child_bool(node, "triggerOnNoteRequest"),
            },
            GITHUB_PUSH_TRIGGER => Self::GitHubPush,
            _ => Self::Other {
                class: class.to_owned(),
            },
        }
    }
}

impl JobConfig {
    /// parse a job's `config.xml`
    ///
    /// Triggers are read from the two places Jenkins stores them: directly
    /// under the root for freestyle, matrix and maven jobs, and inside the
    /// `PipelineTriggersJobProperty` for pipeline jobs. Elements this crate
    /// does not know are kept as [`JobTrigger::Other`] rather than rejected,
    /// since every plugin can add its own.
    ///
    /// # Errors
    ///
    /// returns [`crate::Error::XmlParse`] if the document is not well-formed
    /// XML
    pub fn parse(xml: &str) -> Result<Self, crate::Error> {
        let document = roxmltree::Document::parse(xml)?;
        let root = document.root_element();

        let pipeline_triggers = child(root, "properties")
            .into_iter()
            .flat_map(|properties| {
                properties
                    .children()
                    .filter(|property| property.tag_name().name() == PIPELINE_TRIGGERS_PROPERTY)
            })
            .filter_map(|property| child(property, "triggers"));
        let triggers = child(root, "triggers")
            .into_iter()
            .chain(pipeline_triggers)
            .flat_map(|triggers| triggers.children().filter(roxmltree::Node::is_element))
            .map(JobTrigger::from_node)
            .collect();

        Ok(Self {
            kind: root.tag_name().name().to_owned(),
            disabled: child_bool(root, "disabled"),
            triggers,
        })
    }

    /// whether this is a pipeline job rather than a freestyle, matrix or
    /// maven job
    #[must_use]
    pub fn is_pipeline(&self) -> bool {
        self.kind == "flow-definition"
    }

    /// whether a push to the job's repository starts a build of it
    ///
    /// This is what separates a job that builds *every* change to a repository
    /// from one that merely checks the repository out — a nightly job, or one
    /// started by an upstream job — even though both report the same git
    /// remote and branch. A disabled job starts on nothing.
    #[must_use]
    pub fn starts_on_push(&self) -> bool {
        !self.disabled && self.triggers.iter().any(JobTrigger::starts_on_push)
    }

    /// the jobs whose builds trigger this one, across all of its upstream
    /// triggers
    pub fn upstream_projects(&self) -> impl Iterator<Item = &JobName> {
        self.triggers.iter().flat_map(|trigger| match trigger {
            JobTrigger::Upstream {
                upstream_projects, ..
            } => upstream_projects.as_slice(),
            JobTrigger::ScmPoll { .. }
            | JobTrigger::Timer { .. }
            | JobTrigger::GitLab { .. }
            | JobTrigger::GitHubPush
            | JobTrigger::Other { .. } => &[],
        })
    }

    /// the cron-style schedules the job fires on by itself, timers and SCM
    /// polling alike
    pub fn schedules(&self) -> impl Iterator<Item = &str> {
        self.triggers.iter().filter_map(JobTrigger::schedule)
    }
}

/// the first child element of `node` with the given name
fn child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children()
        .find(|candidate| candidate.is_element() && candidate.tag_name().name() == name)
}

/// the text of the first child element of `node` with the given name
fn child_text<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).and_then(|element| element.text())
}

/// the trimmed text of the first child element of `node` with the given name,
/// `None` when it is absent or blank
fn child_nonempty_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    child_text(node, name)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// the boolean value of the first child element of `node` with the given name,
/// `false` when it is absent, as Jenkins treats an absent flag
fn child_bool(node: roxmltree::Node<'_, '_>, name: &str) -> bool {
    child_text(node, name).is_some_and(|text| text.trim() == "true")
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::error::Error;

    #[test]
    fn test_scm_poll_ignoring_hooks_still_starts_on_push() {
        let trigger = JobTrigger::ScmPoll {
            schedule: Some("H/15 * * * *".to_owned()),
            ignore_post_commit_hooks: true,
        };
        assert!(
            trigger.starts_on_push(),
            "the next poll picks the push up even without the notification"
        );
    }

    #[test]
    fn test_gitlab_trigger_starts_on_push_only_when_configured() {
        let merge_requests_only = JobTrigger::GitLab {
            on_push: false,
            on_merge_request: true,
            on_note: true,
        };
        assert!(!merge_requests_only.starts_on_push(), "push events are off");
    }

    #[test]
    fn test_disabled_job_does_not_start_on_push() -> Result<(), Box<dyn Error>> {
        let config = JobConfig::parse(
            "<project><disabled>true</disabled><triggers>\
             <hudson.triggers.SCMTrigger><spec/></hudson.triggers.SCMTrigger>\
             </triggers></project>",
        )?;
        assert!(!config.starts_on_push(), "a disabled job starts on nothing");
        Ok(())
    }

    #[test]
    fn test_blank_schedule_is_none() -> Result<(), Box<dyn Error>> {
        let config = JobConfig::parse(
            "<project><triggers>\
             <hudson.triggers.SCMTrigger><spec>  </spec></hudson.triggers.SCMTrigger>\
             </triggers></project>",
        )?;
        assert_eq!(
            config.triggers,
            vec![JobTrigger::ScmPoll {
                schedule: None,
                ignore_post_commit_hooks: false,
            }]
        );
        assert_eq!(config.schedules().count(), 0, "a blank spec is no schedule");
        Ok(())
    }

    #[test]
    fn test_unknown_trigger_is_kept_by_class_name() -> Result<(), Box<dyn Error>> {
        let config =
            JobConfig::parse("<project><triggers><org.example.SomeTrigger/></triggers></project>")?;
        assert_eq!(
            config.triggers,
            vec![JobTrigger::Other {
                class: "org.example.SomeTrigger".to_owned()
            }]
        );
        assert_eq!(
            config.triggers.first().map(JobTrigger::class_name),
            Some("org.example.SomeTrigger")
        );
        assert!(
            !config.starts_on_push(),
            "an unknown trigger is not assumed to fire on push"
        );
        Ok(())
    }

    #[test]
    fn test_class_name_round_trips_known_triggers() -> Result<(), Box<dyn Error>> {
        for class in [
            SCM_TRIGGER,
            TIMER_TRIGGER,
            REVERSE_BUILD_TRIGGER,
            GITLAB_TRIGGER,
            GITHUB_PUSH_TRIGGER,
        ] {
            let config = JobConfig::parse(&format!(
                "<project><triggers><{class}/></triggers></project>"
            ))?;
            assert_eq!(
                config.triggers.first().map(JobTrigger::class_name),
                Some(class),
                "class name of the parsed {class} trigger"
            );
            assert!(
                !matches!(config.triggers.first(), Some(JobTrigger::Other { .. })),
                "{class} should parse to a dedicated variant"
            );
        }
        Ok(())
    }

    #[test]
    fn test_malformed_xml_is_an_error() {
        assert!(
            matches!(
                JobConfig::parse("<project><triggers></project>"),
                Err(crate::Error::XmlParse(_))
            ),
            "mismatched tags must not parse"
        );
    }
}
