//! The offline-mode player UUID (brain/wire/wire-offline-login.md).

use md5::{Digest, Md5};

/// Java's `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`: MD5, then the version-3 and IETF
/// variant bits. A pure function of the name.
pub fn offline_uuid(name: &str) -> [u8; 16] {
    let mut h: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    h
}

/// The usual hyphenated form.
pub fn hyphenated(u: &[u8; 16]) -> String {
    let hex: String = u.iter().map(|b| format!("{b:02x}")).collect();
    let part = |a: usize, b: usize| hex.get(a..b).unwrap_or_default().to_owned();
    format!(
        "{}-{}-{}-{}-{}",
        part(0, 8),
        part(8, 12),
        part(12, 16),
        part(16, 20),
        part(20, 32)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notch_offline_uuid() {
        // The well-known offline UUID of the name Notch.
        assert_eq!(
            hyphenated(&offline_uuid("Notch")),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }
}
