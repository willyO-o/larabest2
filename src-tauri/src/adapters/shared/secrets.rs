use rand_core::{OsRng, RngCore};

use crate::ports::SecretSource;

/// Caracteres seguros dentro de comillas simples de PHP (sin `'` ni `\`).
const CHARSET: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&()*+,-.:;<=>?@[]^_{|}~";

/// Aleatoriedad del sistema operativo.
pub struct OsSecrets;

impl SecretSource for OsSecrets {
    fn random_string(&self, len: usize) -> String {
        let mut bytes = vec![0u8; len];
        OsRng.fill_bytes(&mut bytes);
        bytes
            .into_iter()
            .map(|b| CHARSET[b as usize % CHARSET.len()] as char)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_have_length_and_no_quotes() {
        let a = OsSecrets.random_string(64);
        let b = OsSecrets.random_string(64);
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert!(!a.contains('\'') && !a.contains('\\'));
    }
}
