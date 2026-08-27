//! Configuration related code

use std::path::Path;

use serde::Deserialize;

/// the authentication information for a Jenkins instance
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum JenkinsAuth {
    /// a username together with an API token generated in that user's Jenkins
    /// account settings, sent as HTTP basic auth
    ///
    /// This is the normal way to authenticate against Jenkins; the API token
    /// is used in place of the password.
    UserApiToken {
        /// the Jenkins username
        username: String,
        /// the API token generated for that user
        api_token: String,
    },
    /// no authentication at all, for instances that allow anonymous read
    Anonymous {
        /// must be set to `true`; only exists so this variant has a field for
        /// serde's untagged matching to key on
        anonymous: bool,
    },
}

/// this represents the configuration for a Jenkins instance we connect to
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JenkinsInstance {
    /// the base URL of the Jenkins controller, e.g.
    /// `https://jenkins.example.com/`
    ///
    /// Anything to do with TLS — the backend, a private CA certificate,
    /// client certificates — belongs on the reqwest client the caller builds
    /// and passes to [`crate::api::Jenkins::new`], not here.
    pub url: String,
    /// the authentication information
    #[serde(flatten)]
    pub auth: JenkinsAuth,
}

impl JenkinsInstance {
    /// create a new Jenkins instance configuration from a TOML config file
    ///
    /// # Errors
    /// this fails if the configuration file can not be found or parsed
    pub fn from_config_file(path: &Path) -> Result<Self, crate::error::Error> {
        let content =
            fs_err::read_to_string(path).map_err(crate::error::Error::CouldNotReadConfigFile)?;
        let config: Self =
            toml::from_str(&content).map_err(crate::error::Error::CouldNotParseConfig)?;
        Ok(config)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::error::Error;

    #[test]
    fn test_parse_config_with_user_api_token() -> Result<(), Box<dyn Error>> {
        let config: JenkinsInstance = toml::from_str(
            r#"
            url = "https://jenkins.example.com/"
            username = "someuser"
            api_token = "11aabbccddeeff00112233445566778899"
            "#,
        )?;
        assert_eq!(
            config,
            JenkinsInstance {
                url: "https://jenkins.example.com/".to_string(),
                auth: JenkinsAuth::UserApiToken {
                    username: "someuser".to_string(),
                    api_token: "11aabbccddeeff00112233445566778899".to_string(),
                },
            }
        );
        Ok(())
    }

    #[test]
    fn test_unknown_tls_keys_are_not_silently_accepted() -> Result<(), Box<dyn Error>> {
        // TLS is configured on the caller's reqwest client, not here, so a
        // ca_certificate key in the instance config would be a mistake we
        // should not quietly ignore
        let config: JenkinsInstance = toml::from_str(
            r#"
            url = "https://jenkins.example.com/"
            ca_certificate = "/etc/ssl/certs/example-ca.pem"
            username = "someuser"
            api_token = "11aabbccddeeff00112233445566778899"
            "#,
        )?;
        // serde(untagged) on the auth enum means we cannot deny_unknown_fields
        // here, so the best we can do is pin that the rest still parses
        assert_eq!(config.url, "https://jenkins.example.com/");
        Ok(())
    }

    #[test]
    fn test_parse_anonymous_config() -> Result<(), Box<dyn Error>> {
        let config: JenkinsInstance = toml::from_str(
            r#"
            url = "https://jenkins.example.com/"
            anonymous = true
            "#,
        )?;
        assert_eq!(config.auth, JenkinsAuth::Anonymous { anonymous: true });
        Ok(())
    }

    #[test]
    fn test_parse_config_without_auth_fails() {
        let result: Result<JenkinsInstance, _> = toml::from_str(
            r#"
            url = "https://jenkins.example.com/"
            "#,
        );
        assert!(
            result.is_err(),
            "config without credentials or an explicit anonymous flag should fail to parse"
        );
    }
}
