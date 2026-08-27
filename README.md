# jenkins-client

Rust client library for the Jenkins REST API.

The crate concerns itself with **triggering jobs, reading job results and
waiting for jobs**. Configuring Jenkins is deliberately out of scope — that is
owned by Puppet and the Job DSL — so there is no endpoint here that creates or
edits a job.

Both a blocking (`Jenkins`) and an async (`JenkinsAsync`) client are provided
and always compiled; there are no feature flags to choose between them.

## TLS is your choice, not this crate's

This crate depends on reqwest with `default-features = false` and enables no
TLS backend at all, so it forces neither `rustls` nor `native-tls` on you — it
does not even pull in a crypto provider. It compiles with no TLS stack present.

The flip side is that **you** build the reqwest client and pass it in, which is
also where you pick the TLS backend. Add reqwest as your own dependency, enable
what you want, and cargo unifies those features with this crate's:

```toml
[dependencies]
jenkins-client = "0.1"
# rustls pulls in rustls-platform-verifier, so verification uses the system
# trust store and picks up corporate CAs without an OpenSSL build dependency
reqwest = { version = "0.13.4", default-features = false, features = [
  "rustls",
] }
```

Because you own the client, anything else TLS-related — a private CA
certificate, a client certificate, a custom crypto provider — is configured
there too, with reqwest's own API. There is deliberately no `ca_certificate`
setting in this crate's config for that reason.

Two client settings matter to this crate:

- Build it with `.cookie_store(true)` **if you intend to trigger builds**.
  Jenkins' `StrictCrumbIssuer` ties a CSRF crumb to the session it was issued
  for, so without a cookie jar each request gets a fresh session and the crumb
  is rejected. Read-only use does not need it.
- Set a `.user_agent(..)` so your traffic is identifiable in the controller's
  access log.

## What it can do

| Area | Operations |
|---|---|
| Discovery | `jobs`, `jobs_with_scm`, `job`, `downstream_projects` |
| Views | `views`, `view`, `resolve_view` |
| Build results | `builds`, `build`, `last_build`, `console_text` |
| Correlation | `find_build_for_revision`, `find_build_for_upstream` |
| Triggering | `trigger_build`, `queue`, `queue_item` |
| Waiting | `wait_for_build`, `wait_for_queue_item` |
| Diagnostics | `whoami` |

## Views are a tree

With the nested-view plugin a view can contain other views, so `views` returns
a forest rather than a flat list and `View::all_jobs` walks it. Jenkins' `tree`
parameter has no recursion, so the nesting is spelled out to a bounded depth —
one request instead of one per view.

`resolve_view` takes either a full `Parent/Child` path or a bare name. A bare
name is searched for anywhere in the tree, because that is what a person types;
if it matches more than one view the call fails and lists the candidates rather
than guessing, since silently picking one would filter against the wrong jobs.

## Correlating a build to a commit

There are two mechanisms and you need both, because they apply to different
kinds of job:

- Jobs bound to an SCM record the commit they checked out, so
  `find_build_for_revision` matches on the git plugin's `lastBuiltRevision`.
- Jobs triggered by an upstream job — a package-publish job, say — record **no
  git data at all**. For those, `find_build_for_upstream` matches on the
  upstream cause naming the job and build number that triggered them.

## Finding the jobs that build a repository

`jobs_with_scm` returns every job on the controller together with its git
remote URLs and branches in a single request. Matching a repository against
those remote URLs is far more reliable than trying to parse the job naming
convention, which tends not to be one.

## What it deliberately does not do

There is no `notifyCommit` equivalent. Where a forge already notifies Jenkins
on push, a second notification is at best redundant, and several notifications
arriving in quick succession for the same repository can make Jenkins start the
same job more than once.

## Usage

```rust,no_run
use jenkins_client::{Error, JenkinsAsync, JobName, Revision};

/// you build the reqwest client, so you choose the TLS backend
fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::ClientBuilder::new()
        .user_agent("my-tool/1.0")
        // only needed if you trigger builds; see above
        .cookie_store(true)
        .build()
}

/// which jobs build this repository, and on which branches?
async fn jobs_building(remote: &str) -> Result<(), Error> {
    let path = std::path::Path::new("jenkins_instance.toml");
    let jenkins = JenkinsAsync::from_config_file(client()?, path)?;
    for job in jenkins.jobs_with_scm().await? {
        let Some(scm) = &job.scm else { continue };
        let builds_it = scm
            .user_remote_configs
            .iter()
            .any(|config| config.url.as_deref() == Some(remote));
        if builds_it {
            println!("{:?} builds {:?}", job.name, scm.branches);
        }
    }
    Ok(())
}

/// did a specific commit build successfully?
async fn build_for(name: &JobName, revision: &Revision) -> Result<(), Error> {
    let path = std::path::Path::new("jenkins_instance.toml");
    let jenkins = JenkinsAsync::from_config_file(client()?, path)?;
    let found = jenkins
        .find_build_for_revision(name, revision, None)
        .await?;
    if let Some(build) = found {
        println!("{:?} -> {:?}", build.number, build.result);
    }
    Ok(())
}
```

## Configuration

The client is built from a TOML file:

```toml
url = "https://jenkins.example.com/"
username = "someuser"
api_token = "the API token from the user's Jenkins account settings"
```

Anonymous access is also supported, for controllers that allow unauthenticated
reads:

```toml
url = "https://jenkins.example.com/"
anonymous = true
```

Note there is no TLS-related key here; see "TLS is your choice" above.

Read-only `GET` requests are sent straight through. State-changing requests
carry a CSRF crumb, which is fetched lazily, cached, shared between clones of
the client and refreshed once if Jenkins rejects it.

## Tests

`cargo test` runs the unit tests and the offline fixture tests, which replay
captured responses from a real controller through the production parsing code.

The live tests are all `#[ignore]`d and additionally skip themselves when
`JENKINS_TEST_INSTANCE_CONFIG` is unset, so `cargo test -- --include-ignored`
passes on a machine with no Jenkins access. To run them for real, point that
variable at an instance config file (directly or via a gitignored `.env`) and
run `cargo test -- --ignored`.
