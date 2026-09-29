use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use scrypt::{scrypt, Params};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"CDXV";

pub fn codex_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE").ok_or("USERPROFILE is unavailable")?;
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME").ok_or("HOME is unavailable")?;
    Ok(PathBuf::from(home).join(".codex"))
}

fn vault_key() -> Result<[u8; 32], String> {
    #[cfg(windows)]
    let values = (
        std::env::var("USERPROFILE").unwrap_or_default(),
        std::env::var("COMPUTERNAME").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default(),
    );
    #[cfg(not(windows))]
    let values = (
        std::env::var("HOME").unwrap_or_default(),
        sysinfo::System::host_name().unwrap_or_default(),
        std::env::var("USER").unwrap_or_default(),
    );
    derive_key(&values.0, &values.1, &values.2)
}

fn derive_key(userprofile: &str, computername: &str, username: &str) -> Result<[u8; 32], String> {
    let entropy = format!(
        "{}##{}##{}##CodexSwitcher_Vault_Key_9b2e7c4f1a",
        userprofile, computername, username
    );
    let params = Params::new(14, 8, 1, 32).map_err(|e| e.to_string())?;
    let mut key = [0u8; 32];
    scrypt(
        entropy.as_bytes(),
        b"codex_vault_salt_secure_2026",
        &params,
        &mut key,
    )
    .map_err(|e| e.to_string())?;
    Ok(key)
}

pub fn decrypt_data(bytes: &[u8]) -> Result<Value, String> {
    decrypt_with_key(bytes, &vault_key()?)
}

fn decrypt_with_key(bytes: &[u8], key: &[u8; 32]) -> Result<Value, String> {
    if !bytes.starts_with(MAGIC) {
        return serde_json::from_slice(bytes).map_err(|e| e.to_string());
    }
    if bytes.len() < 48 {
        return Err("Encrypted data is incomplete".into());
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(&bytes[4..16]);
    let mut ciphertext = bytes[32..].to_vec();
    ciphertext.extend_from_slice(&bytes[16..32]);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| "Could not decrypt saved data on this user and machine".to_string())?;
    serde_json::from_slice(&plaintext).map_err(|e| e.to_string())
}

pub fn encrypt_data(value: &Value) -> Result<Vec<u8>, String> {
    let key = vault_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let plaintext = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let mut encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| "Could not encrypt saved data".to_string())?;
    let tag = encrypted.split_off(encrypted.len() - 16);
    let mut result = Vec::with_capacity(32 + encrypted.len());
    result.extend_from_slice(MAGIC);
    result.extend_from_slice(&nonce);
    result.extend_from_slice(&tag);
    result.extend_from_slice(&encrypted);
    Ok(result)
}

pub fn read(path: &Path) -> Result<Option<Value>, String> {
    match fs::read(path) {
        Ok(data) => decrypt_data(&data).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub fn write(path: &Path, value: &Value) -> Result<(), String> {
    atomic_write_bytes(path, &encrypt_data(value)?)
}

pub fn atomic_write_bytes(path: &Path, data: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Saved data path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temp = path.with_extension(format!("tmp-{}-{unique}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|e| e.to_string())?;
    file.write_all(data).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    #[cfg(windows)]
    if path.exists() {
        // ReplaceFileW preserves the previous file if replacement fails.
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::ReplaceFileW;
        let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            ReplaceFileW(
                PCWSTR(target.as_ptr()),
                PCWSTR(source.as_ptr()),
                None,
                Default::default(),
                None,
                None,
            )
            .map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_format_round_trips() {
        let input = serde_json::json!({"password": "fixture-secret", "accounts": [1, 2]});
        let encoded = encrypt_data(&input).unwrap();
        assert_eq!(&encoded[..4], MAGIC);
        assert!(!encoded.windows(14).any(|w| w == b"fixture-secret"));
        assert_eq!(decrypt_data(&encoded).unwrap(), input);
    }

    #[test]
    fn decrypts_node_crypto_fixture() {
        // Generated by Node crypto.scryptSync and createCipheriv using dummy machine values.
        let hex = "434458560101010101010101010101015ea89760a2b2363320f99a29ac90924ebdfda942b4069bdc224b88ed3d9beb8a17bcabfa63c0682b0c06289f28f3c03d4dea660bbe3b4bd0b68b9cf3c66f21b2bbdc1f2feb0c8dccc3433dc144cf";
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect();
        let key = derive_key("fixture-home", "fixture-machine", "fixture-user").unwrap();
        assert_eq!(
            decrypt_with_key(&bytes, &key).unwrap(),
            serde_json::json!({"routerUrl": "https://example.test", "password": "fixture-only"})
        );
    }
}
