//! 비밀값(AI API 키, GitHub 토큰) 저장소. ADR-0003.
//!
//! 비밀값은 OS 키체인에만 저장한다. 키체인을 쓸 수 없으면 평문 파일에 쓰지 않고
//! 실행 중인 프로세스의 메모리에만 둔다.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

pub const GITHUB_PAT_KEY: &str = "github_pat";
const AI_API_KEY_PREFIX: &str = "ai_api_key__";
/// 값이 키체인에 있다는 뜻으로 DB 설정 칸에 남기는 표시. 비밀값 자체는 DB에 쓰지 않는다.
pub const CONFIGURED_MARKER: &str = "keychain";
const KEYCHAIN_SERVICE: &str = "skills-manage";
const KEYCHAIN_PROBE_KEY: &str = "keychain-probe";

/// 비밀값이 쓰이는 설정 키인지 확인한다.
pub fn is_secret_key(key: &str) -> bool {
    key == GITHUB_PAT_KEY || key.starts_with(AI_API_KEY_PREFIX)
}

/// 비밀값 저장 인터페이스. 운영은 OS 키체인, 테스트는 메모리 구현을 쓴다.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    /// 없는 항목을 지우는 것은 오류가 아니다.
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// macOS 키체인, Windows 자격 증명 관리자, Linux Secret Service.
pub struct KeychainStore;

impl KeychainStore {
    fn entry(key: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, key).map_err(|error| error.to_string())
    }
}

impl SecretStore for KeychainStore {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        Self::entry(key)?
            .set_password(value)
            .map_err(|error| error.to_string())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// 프로세스 메모리에만 두는 구현. 테스트와 키체인이 없는 환경의 대체 저장소로 쓴다.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

/// 키체인을 우선 쓰고, 쓸 수 없으면 메모리에만 두는 비밀값 저장소.
pub struct Secrets {
    keychain: Arc<dyn SecretStore>,
    memory: MemoryStore,
}

impl Secrets {
    pub fn new(keychain: Arc<dyn SecretStore>) -> Self {
        Self {
            keychain,
            memory: MemoryStore::default(),
        }
    }

    /// 키체인에 저장하고 다시 읽어 같은 값임을 확인했으면 true,
    /// 쓸 수 없거나 확인하지 못해 메모리에만 두었으면 false.
    pub fn set(&self, key: &str, value: &str) -> bool {
        let stored = self.keychain.set(key, value).is_ok()
            && matches!(self.keychain.get(key), Ok(Some(saved)) if saved == value);
        if stored {
            let _ = self.memory.delete(key);
            true
        } else {
            let _ = self.memory.set(key, value);
            false
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match self.keychain.get(key) {
            Ok(Some(value)) => Some(value),
            _ => self.memory.get(key).ok().flatten(),
        }
    }

    pub fn delete(&self, key: &str) -> Result<(), String> {
        let _ = self.memory.delete(key);
        // 키체인을 쓸 수 없으면 지울 항목도 없다. 쓸 수 있는데 실패하면 알린다.
        self.keychain.delete(key).or_else(|error| {
            if self.keychain_available() {
                Err(error)
            } else {
                Ok(())
            }
        })
    }

    pub fn keychain_available(&self) -> bool {
        self.keychain.get(KEYCHAIN_PROBE_KEY).is_ok()
    }
}

/// 앱 전체가 공유하는 비밀값 저장소. 테스트 빌드는 실제 키체인 대신 메모리 구현을 쓴다.
pub fn shared() -> &'static Secrets {
    static SHARED: LazyLock<Secrets> = LazyLock::new(|| {
        if cfg!(any(test, feature = "test-isolation")) {
            Secrets::new(Arc::new(MemoryStore::default()))
        } else {
            Secrets::new(Arc::new(KeychainStore))
        }
    });
    &SHARED
}

/// 키체인이 없는 환경을 흉내 내는 테스트용 구현.
#[cfg(test)]
pub(crate) struct UnavailableStore;

#[cfg(test)]
impl SecretStore for UnavailableStore {
    fn get(&self, _key: &str) -> Result<Option<String>, String> {
        Err("keychain unavailable".to_string())
    }

    fn set(&self, _key: &str, _value: &str) -> Result<(), String> {
        Err("keychain unavailable".to_string())
    }

    fn delete(&self, _key: &str) -> Result<(), String> {
        Err("keychain unavailable".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_to_the_keychain_when_it_is_available() {
        let keychain = Arc::new(MemoryStore::default());
        let secrets = Secrets::new(keychain.clone());

        assert!(secrets.set("github_pat", "ghp_secret"));
        assert_eq!(
            keychain.get("github_pat").unwrap().as_deref(),
            Some("ghp_secret")
        );
        assert_eq!(secrets.get("github_pat").as_deref(), Some("ghp_secret"));
        assert!(secrets.keychain_available());

        secrets.delete("github_pat").unwrap();
        assert_eq!(keychain.get("github_pat").unwrap(), None);
        assert_eq!(secrets.get("github_pat"), None);
    }

    #[test]
    fn keeps_the_value_only_in_memory_when_the_keychain_is_unavailable() {
        let secrets = Secrets::new(Arc::new(UnavailableStore));

        assert!(!secrets.set("github_pat", "ghp_secret"));
        assert_eq!(secrets.get("github_pat").as_deref(), Some("ghp_secret"));
        assert!(!secrets.keychain_available());

        secrets.delete("github_pat").unwrap();
        assert_eq!(secrets.get("github_pat"), None);
    }
}
