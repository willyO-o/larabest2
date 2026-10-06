use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::ServiceId;

/// Una versión copiada en `bin/<servicio>/<versión>/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    pub service: ServiceId,
    pub version: String,
    pub path: String,
}

/// Compara versiones por sus partes numéricas ("8.10.0" > "8.9.12").
/// Lo que no es número se compara como texto.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts = |v: &str| -> Vec<String> {
        v.split(['.', '-', '_'])
            .map(|p| p.to_ascii_lowercase())
            .collect()
    };
    let (pa, pb) = (parts(a), parts(b));
    for (x, y) in pa.iter().zip(pb.iter()) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(nx), Ok(ny)) => nx.cmp(&ny),
            _ => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    pa.len().cmp(&pb.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numerically() {
        assert_eq!(compare_versions("8.10.0", "8.9.12"), Ordering::Greater);
        assert_eq!(compare_versions("8.3", "8.3.0"), Ordering::Less);
        assert_eq!(compare_versions("11.4.5", "11.4.5"), Ordering::Equal);
    }
}
