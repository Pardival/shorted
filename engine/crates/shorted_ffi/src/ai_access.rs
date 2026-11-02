//! What the AI providers need from the device: the key in the system keychain, the non-secret
//! settings in a file, and HTTP. Kept out of `api` so these types stay out of the bridge.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use shorted_ai::providers::{
    AiError, Http, HttpRequest, HttpResponse, Provider, ProviderConfig, ProviderKind, Secret,
};

use crate::error::EngineError;

/// The keychain entry's service; the account is the provider's id.
#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
const KEYCHAIN_SERVICE: &str = "app.shorted.ai";
const SETTINGS_FILE: &str = "ai.json";

/// Everything but the key, which never goes to a file (docs/08).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AiSettings {
    pub provider: Option<String>,
    pub model: String,
    pub base_url: String,
}

fn settings_file(root: &Path) -> PathBuf {
    root.join(SETTINGS_FILE)
}

pub(crate) fn load_settings(root: &Path) -> AiSettings {
    std::fs::read(settings_file(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub(crate) fn save_settings(root: &Path, settings: &AiSettings) -> Result<(), EngineError> {
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|_| EngineError::Internal("settings not written".into()))?;
    std::fs::create_dir_all(root)?;
    std::fs::write(settings_file(root), bytes)?;
    Ok(())
}

/// Where keys live. The system keychain in the app; memory in tests.
pub(crate) trait KeyStore {
    fn get(&self, provider: &str) -> Option<Secret>;
    fn set(&self, provider: &str, key: &Secret) -> Result<(), EngineError>;
    fn delete(&self, provider: &str);
}

pub(crate) struct SystemKeychain;

#[cfg(target_vendor = "apple")]
impl KeyStore for SystemKeychain {
    fn get(&self, provider: &str) -> Option<Secret> {
        let bytes =
            security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, provider).ok()?;
        String::from_utf8(bytes).ok().map(Secret::new)
    }

    fn set(&self, provider: &str, key: &Secret) -> Result<(), EngineError> {
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            provider,
            key.expose().as_bytes(),
        )
        .map_err(|_| EngineError::KeyStorage)
    }

    fn delete(&self, provider: &str) {
        // Nothing stored is fine: the key is gone either way.
        let _ = security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, provider);
    }
}

/// No secure store here yet: a key is refused rather than written anywhere else.
#[cfg(not(target_vendor = "apple"))]
impl KeyStore for SystemKeychain {
    fn get(&self, _provider: &str) -> Option<Secret> {
        None
    }

    fn set(&self, _provider: &str, _key: &Secret) -> Result<(), EngineError> {
        Err(EngineError::KeyStorage)
    }

    fn delete(&self, _provider: &str) {}
}

/// The provider the settings describe, with its key, if both are there.
pub(crate) fn configured(root: &Path, keys: &dyn KeyStore) -> Result<Provider, EngineError> {
    let settings = load_settings(root);
    let kind = settings
        .provider
        .as_deref()
        .and_then(ProviderKind::from_id)
        .ok_or(EngineError::AiNotConfigured)?;
    let key = keys.get(kind.id()).ok_or(EngineError::AiNotConfigured)?;
    let model = if settings.model.trim().is_empty() {
        kind.default_model().to_string()
    } else {
        settings.model
    };
    Ok(Provider::new(
        ProviderConfig {
            kind,
            model,
            base_url: Some(settings.base_url).filter(|url| !url.trim().is_empty()),
        },
        key,
    ))
}

/// The app's HTTP for providers: TLS from the system, 60 s at most (docs/07 §3).
pub(crate) struct DeviceHttp;

#[cfg(target_vendor = "apple")]
impl Http for DeviceHttp {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, AiError> {
        use std::time::Duration;

        use shorted_ai::providers::Method;
        use ureq::tls::{TlsConfig, TlsProvider};

        let tls = TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .build();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .into();
        let result = match request.method {
            Method::Get => {
                let mut call = agent.get(&request.url);
                for (name, value) in &request.headers {
                    call = call.header(name, value);
                }
                call.call()
            }
            Method::Post => {
                let mut call = agent.post(&request.url);
                for (name, value) in &request.headers {
                    call = call.header(name, value);
                }
                call.send(request.body.as_deref().unwrap_or_default())
            }
        };
        // The error may quote the request: only its kind is kept.
        let mut response = result.map_err(|_| AiError::Network)?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|_| AiError::Network)?;
        Ok(HttpResponse { status, body })
    }
}

#[cfg(not(target_vendor = "apple"))]
impl Http for DeviceHttp {
    fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, AiError> {
        Err(AiError::Network)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    pub(crate) struct MemoryKeys(pub RefCell<HashMap<String, Secret>>);

    impl KeyStore for MemoryKeys {
        fn get(&self, provider: &str) -> Option<Secret> {
            self.0.borrow().get(provider).cloned()
        }

        fn set(&self, provider: &str, key: &Secret) -> Result<(), EngineError> {
            self.0.borrow_mut().insert(provider.into(), key.clone());
            Ok(())
        }

        fn delete(&self, provider: &str) {
            self.0.borrow_mut().remove(provider);
        }
    }

    #[test]
    fn without_a_key_or_a_provider_nothing_is_configured() {
        let root = tempfile::tempdir().unwrap();
        let keys = MemoryKeys::default();
        assert!(matches!(
            configured(root.path(), &keys),
            Err(EngineError::AiNotConfigured)
        ));
        save_settings(
            root.path(),
            &AiSettings {
                provider: Some("anthropic".into()),
                model: String::new(),
                base_url: String::new(),
            },
        )
        .unwrap();
        assert!(configured(root.path(), &keys).is_err());
        keys.set("anthropic", &Secret::new("k".into())).unwrap();
        let provider = configured(root.path(), &keys).unwrap();
        assert_eq!(provider.config.model, "claude-sonnet-5-5");
        assert_eq!(provider.config.base_url, None);
    }
}
