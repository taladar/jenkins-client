//! Configuration related code

use std::path::{Path, PathBuf};

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
    pub url: String,
    /// the CA certificate to use to validate the server certificate
    pub ca_certificate: Option<PathBuf>,
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
                ca_certificate: None,
                auth: JenkinsAuth::UserApiToken {
                    username: "someuser".to_string(),
                    api_token: "11aabbccddeeff00112233445566778899".to_string(),
                },
            }
        );
        Ok(())
    }

    #[test]
    fn test_parse_config_with_ca_certificate() -> Result<(), Box<dyn Error>> {
        let config: JenkinsInstance = toml::from_str(
            r#"
            url = "https://jenkins.example.com/"
            ca_certificate = "/etc/ssl/certs/example-ca.pem"
            username = "someuser"
            api_token = "11aabbccddeeff00112233445566778899"
            "#,
        )?;
        assert_eq!(
            config.ca_certificate,
            Some(PathBuf::from("/etc/ssl/certs/example-ca.pem"))
        );
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
