use serde::{Deserialize, Serialize};

use crate::error::{VaultError, VaultResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    Dpapi,
    Tpm,
    Hello,
    Fido2,
}

pub trait KeyBinder: Send + Sync {
    fn id(&self) -> &'static str;
    fn kind(&self) -> BindingKind;
    fn label(&self) -> String;
    fn available(&self) -> bool;
    fn seal(&self, name: &str, data: &[u8]) -> VaultResult<Vec<u8>>;
    fn unseal(&self, name: &str, blob: &[u8]) -> VaultResult<Vec<u8>>;
}

#[derive(Debug, Default)]
pub struct MockBinder {
    pub fail: std::sync::atomic::AtomicBool,
}

impl MockBinder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyBinder for MockBinder {
    fn id(&self) -> &'static str {
        "mock"
    }
    fn kind(&self) -> BindingKind {
        BindingKind::Dpapi
    }
    fn label(&self) -> String {
        "Test binder".into()
    }
    fn available(&self) -> bool {
        true
    }
    fn seal(&self, _name: &str, data: &[u8]) -> VaultResult<Vec<u8>> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(VaultError::BindingFailed);
        }
        let mut out = b"ROFMOCK".to_vec();
        out.extend_from_slice(data);
        Ok(out)
    }
    fn unseal(&self, _name: &str, blob: &[u8]) -> VaultResult<Vec<u8>> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(VaultError::BindingFailed);
        }
        blob.strip_prefix(b"ROFMOCK".as_slice())
            .map(|b| b.to_vec())
            .ok_or(VaultError::BindingFailed)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ptr;
    use zeroize::Zeroize;
    use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
    }

    fn take_blob(out: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
        unsafe {
            let slice = std::slice::from_raw_parts(out.pbData, out.cbData as usize);
            let v = slice.to_vec();
            LocalFree(out.pbData as HLOCAL);
            v
        }
    }

    pub fn dpapi_protect(name: &str, data: &[u8]) -> VaultResult<Vec<u8>> {
        let description = wide(name);
        let mut entropy = name.as_bytes().to_vec();
        let entropy_blob = blob(&entropy);
        let input = blob(data);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        let ok = unsafe {
            CryptProtectData(
                &input,
                description.as_ptr(),
                &entropy_blob,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        entropy.zeroize();
        if ok == 0 {
            return Err(VaultError::BindingFailed);
        }
        Ok(take_blob(&out))
    }

    pub fn dpapi_unprotect(name: &str, data: &[u8]) -> VaultResult<Vec<u8>> {
        let mut entropy = name.as_bytes().to_vec();
        let entropy_blob = blob(&entropy);
        let input = blob(data);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy_blob,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        entropy.zeroize();
        if ok == 0 {
            return Err(VaultError::BindingFailed);
        }
        Ok(take_blob(&out))
    }

    #[derive(Debug, Default)]
    pub struct DpapiBinder;

    impl KeyBinder for DpapiBinder {
        fn id(&self) -> &'static str {
            "dpapi-user"
        }
        fn kind(&self) -> BindingKind {
            BindingKind::Dpapi
        }
        fn label(&self) -> String {
            "Windows account (DPAPI)".into()
        }
        fn available(&self) -> bool {
            true
        }
        fn seal(&self, name: &str, data: &[u8]) -> VaultResult<Vec<u8>> {
            dpapi_protect(name, data)
        }
        fn unseal(&self, name: &str, blob: &[u8]) -> VaultResult<Vec<u8>> {
            dpapi_unprotect(name, blob)
        }
    }

    use windows_sys::Win32::Security::Cryptography::{
        NCryptCreatePersistedKey, NCryptDecrypt, NCryptDeleteKey, NCryptEncrypt, NCryptFinalizeKey,
        NCryptFreeObject, NCryptOpenKey, NCryptOpenStorageProvider, NCryptSetProperty, NCRYPT_KEY_HANDLE,
        NCRYPT_PROV_HANDLE,
    };

    const NCRYPT_PAD_PKCS1_FLAG: u32 = 0x0000_0002;
    const NCRYPT_UI_POLICY_PROPERTY: &str = "UI Policy";
    const NCRYPT_LENGTH_PROPERTY: &str = "Length";

    #[repr(C)]
    struct NcryptUiPolicy {
        dw_version: u32,
        dw_flags: u32,
        psz_creation_title: *const u16,
        psz_friendly_name: *const u16,
        psz_description: *const u16,
    }

    pub struct TpmBinder {
        pub hello: bool,
    }

    impl std::fmt::Debug for TpmBinder {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TpmBinder").field("hello", &self.hello).finish()
        }
    }

    fn provider() -> VaultResult<NCRYPT_PROV_HANDLE> {
        let name = wide("Microsoft Platform Crypto Provider");
        let mut handle: NCRYPT_PROV_HANDLE = 0;
        let status = unsafe { NCryptOpenStorageProvider(&mut handle, name.as_ptr(), 0) };
        if status != 0 {
            return Err(VaultError::BindingUnavailable);
        }
        Ok(handle)
    }

    fn open_or_create_key(provider: NCRYPT_PROV_HANDLE, name: &str, hello: bool) -> VaultResult<NCRYPT_KEY_HANDLE> {
        let key_name = wide(name);
        let mut key: NCRYPT_KEY_HANDLE = 0;
        let status = unsafe { NCryptOpenKey(provider, &mut key, key_name.as_ptr(), 0, 0) };
        if status == 0 {
            return Ok(key);
        }
        let alg = wide("RSA");
        let status = unsafe { NCryptCreatePersistedKey(provider, &mut key, alg.as_ptr(), key_name.as_ptr(), 0, 0) };
        if status != 0 {
            return Err(VaultError::BindingUnavailable);
        }
        let mut bits: u32 = 2048;
        let length_prop = wide(NCRYPT_LENGTH_PROPERTY);
        unsafe {
            NCryptSetProperty(
                key,
                length_prop.as_ptr(),
                &mut bits as *mut u32 as *const u8,
                4,
                0,
            );
        }
        if hello {
            let title = wide("RoFarm");
            let friendly = wide("Unlock the RoFarm vault");
            let description = wide("Windows Hello protects your RoFarm vault key.");
            let policy = NcryptUiPolicy {
                dw_version: 1,
                dw_flags: 0x1 | 0x10,
                psz_creation_title: title.as_ptr(),
                psz_friendly_name: friendly.as_ptr(),
                psz_description: description.as_ptr(),
            };
            let prop = wide(NCRYPT_UI_POLICY_PROPERTY);
            unsafe {
                NCryptSetProperty(
                    key,
                    prop.as_ptr(),
                    &policy as *const NcryptUiPolicy as *const u8,
                    std::mem::size_of::<NcryptUiPolicy>() as u32,
                    0,
                );
            }
        }
        let status = unsafe { NCryptFinalizeKey(key, 0) };
        if status != 0 {
            unsafe { NCryptFreeObject(key) };
            return Err(VaultError::BindingUnavailable);
        }
        Ok(key)
    }

    impl KeyBinder for TpmBinder {
        fn id(&self) -> &'static str {
            if self.hello { "tpm-hello" } else { "tpm" }
        }
        fn kind(&self) -> BindingKind {
            if self.hello { BindingKind::Hello } else { BindingKind::Tpm }
        }
        fn label(&self) -> String {
            if self.hello { "TPM 2.0 with Windows Hello".into() } else { "TPM 2.0".into() }
        }
        fn available(&self) -> bool {
            provider().is_ok()
        }
        fn seal(&self, name: &str, data: &[u8]) -> VaultResult<Vec<u8>> {
            let prov = provider()?;
            let key = open_or_create_key(prov, name, self.hello)?;
            let mut needed: u32 = 0;
            let status = unsafe {
                NCryptEncrypt(key, data.as_ptr(), data.len() as u32, std::ptr::null(), std::ptr::null_mut(), 0, &mut needed, NCRYPT_PAD_PKCS1_FLAG)
            };
            if status != 0 || needed == 0 {
                unsafe {
                    NCryptFreeObject(key);
                    NCryptFreeObject(prov);
                }
                return Err(VaultError::BindingFailed);
            }
            let mut out = vec![0u8; needed as usize];
            let status = unsafe {
                NCryptEncrypt(key, data.as_ptr(), data.len() as u32, std::ptr::null(), out.as_mut_ptr(), needed, &mut needed, NCRYPT_PAD_PKCS1_FLAG)
            };
            unsafe {
                NCryptFreeObject(key);
                NCryptFreeObject(prov);
            }
            if status != 0 {
                out.zeroize();
                return Err(VaultError::BindingFailed);
            }
            out.truncate(needed as usize);
            Ok(out)
        }
        fn unseal(&self, name: &str, blob: &[u8]) -> VaultResult<Vec<u8>> {
            let prov = provider()?;
            let key = {
                let key_name = wide(name);
                let mut key: NCRYPT_KEY_HANDLE = 0;
                let status = unsafe { NCryptOpenKey(prov, &mut key, key_name.as_ptr(), 0, 0) };
                if status != 0 {
                    unsafe { NCryptFreeObject(prov) };
                    return Err(VaultError::BindingFailed);
                }
                key
            };
            let mut needed: u32 = 0;
            let status = unsafe {
                NCryptDecrypt(key, blob.as_ptr(), blob.len() as u32, std::ptr::null(), std::ptr::null_mut(), 0, &mut needed, NCRYPT_PAD_PKCS1_FLAG)
            };
            if status != 0 || needed == 0 {
                unsafe {
                    NCryptFreeObject(key);
                    NCryptFreeObject(prov);
                }
                return Err(VaultError::BindingFailed);
            }
            let mut out = vec![0u8; needed as usize];
            let status = unsafe {
                NCryptDecrypt(key, blob.as_ptr(), blob.len() as u32, std::ptr::null(), out.as_mut_ptr(), needed, &mut needed, NCRYPT_PAD_PKCS1_FLAG)
            };
            unsafe {
                NCryptFreeObject(key);
                NCryptFreeObject(prov);
            }
            if status != 0 {
                out.zeroize();
                return Err(VaultError::BindingFailed);
            }
            out.truncate(needed as usize);
            Ok(out)
        }
    }

    impl Drop for TpmBinder {
        fn drop(&mut self) {}
    }

    #[allow(dead_code)]
    pub fn delete_tpm_key(name: &str) {
        if let Ok(prov) = provider() {
            let key_name = wide(name);
            let mut key: NCRYPT_KEY_HANDLE = 0;
            let status = unsafe { NCryptOpenKey(prov, &mut key, key_name.as_ptr(), 0, 0) };
            if status == 0 {
                unsafe {
                    NCryptDeleteKey(key, 0);
                }
            }
            unsafe {
                NCryptFreeObject(prov);
            }
        }
    }
}

#[cfg(windows)]
pub use imp::{DpapiBinder, TpmBinder};

#[cfg(not(windows))]
mod imp_portable {
    use super::*;

    #[derive(Debug, Default)]
    pub struct DpapiBinder;

    impl KeyBinder for DpapiBinder {
        fn id(&self) -> &'static str {
            "dpapi-user"
        }
        fn kind(&self) -> BindingKind {
            BindingKind::Dpapi
        }
        fn label(&self) -> String {
            "Windows account (unavailable)".into()
        }
        fn available(&self) -> bool {
            false
        }
        fn seal(&self, _name: &str, _data: &[u8]) -> VaultResult<Vec<u8>> {
            Err(VaultError::BindingUnavailable)
        }
        fn unseal(&self, _name: &str, _blob: &[u8]) -> VaultResult<Vec<u8>> {
            Err(VaultError::BindingUnavailable)
        }
    }

    #[derive(Debug, Default)]
    pub struct TpmBinder {
        pub hello: bool,
    }

    impl KeyBinder for TpmBinder {
        fn id(&self) -> &'static str {
            if self.hello { "tpm-hello" } else { "tpm" }
        }
        fn kind(&self) -> BindingKind {
            if self.hello { BindingKind::Hello } else { BindingKind::Tpm }
        }
        fn label(&self) -> String {
            "TPM 2.0 (unavailable)".into()
        }
        fn available(&self) -> bool {
            false
        }
        fn seal(&self, _name: &str, _data: &[u8]) -> VaultResult<Vec<u8>> {
            Err(VaultError::BindingUnavailable)
        }
        fn unseal(&self, _name: &str, _blob: &[u8]) -> VaultResult<Vec<u8>> {
            Err(VaultError::BindingUnavailable)
        }
    }
}

#[cfg(not(windows))]
pub use imp_portable::{DpapiBinder, TpmBinder};

#[derive(Debug, Default)]
pub struct Fido2Binder;

impl KeyBinder for Fido2Binder {
    fn id(&self) -> &'static str {
        "fido2"
    }
    fn kind(&self) -> BindingKind {
        BindingKind::Fido2
    }
    fn label(&self) -> String {
        "Security key (not implemented)".into()
    }
    fn available(&self) -> bool {
        false
    }
    fn seal(&self, _name: &str, _data: &[u8]) -> VaultResult<Vec<u8>> {
        Err(VaultError::BindingUnavailable)
    }
    fn unseal(&self, _name: &str, _blob: &[u8]) -> VaultResult<Vec<u8>> {
        Err(VaultError::BindingUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_binder_round_trips_and_can_fail() {
        let binder = MockBinder::new();
        let sealed = binder.seal("name", b"master").unwrap();
        assert_eq!(binder.unseal("name", &sealed).unwrap(), b"master");
        binder.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(binder.seal("name", b"master").is_err());
        assert!(!binder.unseal("name", &sealed).is_ok());
        assert_eq!(Fido2Binder.available(), false);
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_round_trips_for_current_user() {
        let binder = DpapiBinder;
        assert!(binder.available());
        let sealed = binder.seal("rofarm-test", b"secret material").unwrap();
        let opened = binder.unseal("rofarm-test", &sealed).unwrap();
        assert_eq!(opened, b"secret material");
        assert!(binder.unseal("other-name", &sealed).is_err());
    }
}
