//! AI providers in Settings (T-0805): which one, which model, its key, and a connection test.
//! The key goes to the system keychain only; Dart never reads it back.

use std::path::Path;

use shorted_ai::providers::{ProviderKind, Secret};

use crate::ai_access::{
    configured, load_settings, save_settings, AiSettings, DeviceHttp, KeyStore, SystemKeychain,
};
use crate::api::projects::on_command_thread;
use crate::error::{EngineError, EngineFailure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiProviderView {
    OpenAi,
    Anthropic,
    Gemini,
    OpenAiCompatible,
}

impl From<AiProviderView> for ProviderKind {
    fn from(view: AiProviderView) -> Self {
        match view {
            AiProviderView::OpenAi => Self::OpenAi,
            AiProviderView::Anthropic => Self::Anthropic,
            AiProviderView::Gemini => Self::Gemini,
            AiProviderView::OpenAiCompatible => Self::OpenAiCompatible,
        }
    }
}

fn provider_view(kind: ProviderKind) -> AiProviderView {
    match kind {
        ProviderKind::OpenAi => AiProviderView::OpenAi,
        ProviderKind::Anthropic => AiProviderView::Anthropic,
        ProviderKind::Gemini => AiProviderView::Gemini,
        ProviderKind::OpenAiCompatible => AiProviderView::OpenAiCompatible,
    }
}

pub struct AiSettingsView {
    pub provider: Option<AiProviderView>,
    pub model: String,
    pub base_url: String,
    /// Whether a key is stored; the key itself never comes back.
    pub has_key: bool,
}

fn view(root: &Path, keys: &dyn KeyStore) -> AiSettingsView {
    let settings = load_settings(root);
    let kind = settings.provider.as_deref().and_then(ProviderKind::from_id);
    AiSettingsView {
        provider: kind.map(provider_view),
        model: settings.model,
        base_url: settings.base_url,
        has_key: kind.is_some_and(|kind| keys.get(kind.id()).is_some()),
    }
}

/// The model Shorted suggests for a provider.
#[flutter_rust_bridge::frb(sync)]
pub fn default_ai_model(provider: AiProviderView) -> String {
    ProviderKind::from(provider).default_model().to_string()
}

pub async fn ai_settings(root: String) -> Result<AiSettingsView, EngineFailure> {
    on_command_thread(move || Ok(view(Path::new(&root), &SystemKeychain)))
}

fn save(
    root: &Path,
    keys: &dyn KeyStore,
    provider: AiProviderView,
    model: String,
    base_url: String,
    key: Option<String>,
) -> Result<AiSettingsView, EngineError> {
    let kind = ProviderKind::from(provider);
    if let Some(key) = key
        .map(|key| key.trim().to_string())
        .filter(|k| !k.is_empty())
    {
        keys.set(kind.id(), &Secret::new(key))?;
    }
    save_settings(
        root,
        &AiSettings {
            provider: Some(kind.id().into()),
            model: model.trim().into(),
            base_url: base_url.trim().into(),
        },
    )?;
    Ok(view(root, keys))
}

/// Saves the choice of provider and model; a new key, when given, replaces the stored one.
pub async fn save_ai_settings(
    root: String,
    provider: AiProviderView,
    model: String,
    base_url: String,
    key: Option<String>,
) -> Result<AiSettingsView, EngineFailure> {
    on_command_thread(move || {
        save(
            Path::new(&root),
            &SystemKeychain,
            provider,
            model,
            base_url,
            key,
        )
    })
}

/// Removes the stored key of the chosen provider.
pub async fn forget_ai_key(root: String) -> Result<AiSettingsView, EngineFailure> {
    on_command_thread(move || {
        let root = Path::new(&root);
        if let Some(kind) = load_settings(root)
            .provider
            .as_deref()
            .and_then(ProviderKind::from_id)
        {
            SystemKeychain.delete(kind.id());
        }
        Ok(view(root, &SystemKeychain))
    })
}

/// Lists the provider's models with the stored key: a clear error for a wrong key (401/403),
/// too many requests (429), a provider in trouble, or no network.
pub async fn test_ai_connection(root: String) -> Result<(), EngineFailure> {
    on_command_thread(move || {
        let provider = configured(Path::new(&root), &SystemKeychain)?;
        provider
            .test_connection(&DeviceHttp)
            .map_err(EngineError::Ai)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::ai_access::tests::MemoryKeys;

    const KEY: &str = "sk-ant-secret-0123456789";

    /// Collects everything logged, to check the key is never in it.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn files_under(folder: &Path) -> Vec<Vec<u8>> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(files_under(&path));
            } else {
                found.push(std::fs::read(path).unwrap());
            }
        }
        found
    }

    #[test]
    fn the_key_never_reaches_a_file_or_a_log() {
        let root = tempfile::tempdir().unwrap();
        let keys = MemoryKeys::default();
        let logs = Captured::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_max_level(tracing::Level::TRACE)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let saved = save(
                root.path(),
                &keys,
                AiProviderView::Anthropic,
                " claude-sonnet-5-5 ".into(),
                String::new(),
                Some(format!("  {KEY} ")),
            )
            .unwrap();
            assert!(saved.has_key);
            assert_eq!(saved.model, "claude-sonnet-5-5");
            let provider = configured(root.path(), &keys).unwrap();
            tracing::info!(?provider, "configured");
            let failure: EngineFailure =
                EngineError::Ai(shorted_ai::providers::AiError::KeyRejected).into();
            assert_eq!(failure.code, crate::error::ErrorCode::AiKeyRejected);
        });
        assert_eq!(keys.get("anthropic").unwrap().expose(), KEY);
        let logged = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
        assert!(logged.contains("configured"), "the log was not captured");
        assert!(!logged.contains(KEY), "{logged}");
        for bytes in files_under(root.path()) {
            assert!(!String::from_utf8_lossy(&bytes).contains(KEY));
        }
    }

    #[test]
    fn saving_without_a_key_keeps_the_stored_one() {
        let root = tempfile::tempdir().unwrap();
        let keys = MemoryKeys::default();
        let first = || {
            save(
                root.path(),
                &keys,
                AiProviderView::OpenAi,
                String::new(),
                String::new(),
                Some("k1".into()),
            )
        };
        first().unwrap();
        let again = save(
            root.path(),
            &keys,
            AiProviderView::OpenAi,
            "gpt-x".into(),
            String::new(),
            None,
        )
        .unwrap();
        assert!(again.has_key);
        assert_eq!(keys.get("openai").unwrap().expose(), "k1");
        let other = view(root.path(), &keys);
        assert_eq!(other.provider, Some(AiProviderView::OpenAi));
    }
}
