//! Extension settings, edited on the extension's page in the admin area.
use serde::{Deserialize, Serialize};
use shared::{
    State,
    extensions::settings::{
        ExtensionSettings, SettingsDeserializeExt, SettingsDeserializer, SettingsSerializeExt,
        SettingsSerializer,
    },
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Servers whose owner linked no account of their own sign in with the host account.
    pub host_account: bool,
    /// Users may link their own Hytale account for the servers they own.
    pub user_accounts: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            host_account: true,
            user_accounts: true,
        }
    }
}

#[async_trait::async_trait]
impl SettingsSerializeExt for Settings {
    async fn serialize(
        &self,
        serializer: SettingsSerializer,
    ) -> Result<SettingsSerializer, anyhow::Error> {
        Ok(serializer
            .write_serde_setting("host_account", &self.host_account)?
            .write_serde_setting("user_accounts", &self.user_accounts)?)
    }
}

pub struct SettingsDataDeserializer;

#[async_trait::async_trait]
impl SettingsDeserializeExt for SettingsDataDeserializer {
    async fn deserialize_boxed(
        &self,
        deserializer: SettingsDeserializer<'_>,
    ) -> Result<ExtensionSettings, anyhow::Error> {
        let defaults = Settings::default();
        Ok(Box::new(Settings {
            host_account: deserializer
                .read_serde_setting("host_account")
                .unwrap_or(defaults.host_account),
            user_accounts: deserializer
                .read_serde_setting("user_accounts")
                .unwrap_or(defaults.user_accounts),
        }))
    }
}

pub async fn load(state: &State) -> Result<Settings, anyhow::Error> {
    let settings = state.settings.get().await?;
    Ok(settings
        .get_extension_settings::<Settings>(crate::PACKAGE)
        .cloned()
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_updates_are_refused() {
        // the admin page sends every field; a missing one is an error, not a silent default
        assert!(serde_json::from_str::<Settings>(r#"{"host_account":false}"#).is_err());
        let full: Settings =
            serde_json::from_str(r#"{"host_account":false,"user_accounts":true}"#).unwrap();
        assert!(!full.host_account);
    }
}
