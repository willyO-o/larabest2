//! Comprobar la firma digital de un archivo descargado con la API de
//! Windows (`WinVerifyTrust`), sin PowerShell: la firma es válida, de
//! confianza, y de la organización esperada.

use std::path::Path;

use windows_sys::core::GUID;
use windows_sys::Win32::Security::Cryptography::{
    szOID_ORGANIZATION_NAME, CertGetNameStringW, CERT_NAME_ATTR_TYPE,
};
use windows_sys::Win32::Security::WinTrust::{
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `Ok(true)` si `file` tiene una firma válida y de confianza de
/// `organization` (el campo O= del certificado, como "Microsoft
/// Corporation"). `Err` si no se pudo comprobar.
pub fn signed_by(file: &Path, organization: &str) -> Result<bool, String> {
    let path = wide(&file.to_string_lossy());
    // SAFETY: estructuras simples rellenadas con ceros y los campos que
    // pide WinVerifyTrust; `path` vive hasta el final.
    let mut info: WINTRUST_FILE_INFO = unsafe { std::mem::zeroed() };
    info.cbStruct = std::mem::size_of::<WINTRUST_FILE_INFO>() as u32;
    info.pcwszFilePath = path.as_ptr();
    let mut data: WINTRUST_DATA = unsafe { std::mem::zeroed() };
    data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.Anonymous = WINTRUST_DATA_0 { pFile: &mut info };
    data.dwStateAction = WTD_STATEACTION_VERIFY;
    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;

    // SAFETY: `data` e `info` están bien formadas; sin ventana.
    let status = unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    let result = if status != 0 {
        Ok(false)
    } else {
        // SAFETY: estado devuelto por la verificación que acaba de terminar
        // bien; los punteros se revisan antes de usarlos.
        unsafe { signer_organization(data.hWVTStateData) }
            .map(|org| org.eq_ignore_ascii_case(organization))
            .ok_or_else(|| "no se pudo leer quién firmó el archivo".to_string())
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: libera el estado de la verificación anterior.
    unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    result
}

/// El campo O= del certificado de quien firmó.
unsafe fn signer_organization(state: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
    let provider = WTHelperProvDataFromStateData(state);
    if provider.is_null() {
        return None;
    }
    let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
    if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
        return None;
    }
    let cert = (*(*signer).pasCertChain).pCert;
    if cert.is_null() {
        return None;
    }
    let mut buf = [0u16; 256];
    let len = CertGetNameStringW(
        cert,
        CERT_NAME_ATTR_TYPE,
        0,
        szOID_ORGANIZATION_NAME as *const std::ffi::c_void,
        buf.as_mut_ptr(),
        buf.len() as u32,
    );
    // `len` cuenta el cero final; 1 = vacío.
    (len > 1).then(|| String::from_utf16_lossy(&buf[..len as usize - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Con un archivo firmado de verdad (el `vc_redist` de Microsoft):
    /// `LARABEST_SIGNED_FILE=<ruta> cargo test -- --ignored signed_file`.
    #[test]
    #[ignore = "necesita un archivo firmado por Microsoft"]
    fn recognizes_a_microsoft_signed_file() {
        let file = std::env::var("LARABEST_SIGNED_FILE").expect("LARABEST_SIGNED_FILE");
        let file = Path::new(&file);
        assert_eq!(signed_by(file, "Microsoft Corporation"), Ok(true));
        assert_eq!(signed_by(file, "Otra Empresa"), Ok(false));
    }

    #[test]
    fn an_unsigned_file_is_not_trusted() {
        let dir = crate::adapters::shared::temp_dir("firma");
        let plain = dir.join("sin-firma.exe");
        std::fs::write(&plain, b"MZ no es un programa").unwrap();
        assert_eq!(signed_by(&plain, "Microsoft Corporation"), Ok(false));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
