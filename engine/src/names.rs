pub fn store_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn strip_hash(name: &str) -> &str {
    let bytes = name.as_bytes();
    if bytes.len() > 33 && bytes[32] == b'-' {
        &name[33..]
    } else {
        name
    }
}

pub fn package_base(name: &str) -> &str {
    let name = strip_hash(name);
    let bytes = name.as_bytes();
    let mut start = 1;
    while start < bytes.len() {
        if bytes[start - 1] == b'-' && bytes[start].is_ascii_digit() {
            return &name[..start - 1];
        }
        start += 1;
    }
    name
}

pub fn version_of(name: &str) -> Option<&str> {
    let name = strip_hash(name);
    let bytes = name.as_bytes();
    let mut start = 1;
    while start < bytes.len() {
        if bytes[start - 1] == b'-' && bytes[start].is_ascii_digit() {
            return Some(&name[start..]);
        }
        start += 1;
    }
    None
}

fn strip_extension(name: &str) -> &str {
    if let Some((stem, extension)) = name.rsplit_once('.') {
        if !extension.is_empty()
            && extension.len() <= 6
            && extension.chars().all(|character| character.is_ascii_alphabetic())
        {
            return stem;
        }
    }
    name
}

pub fn pretty_name(path: &str) -> String {
    let base = strip_hash(store_basename(path));
    strip_extension(base).replace('-', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(rest: &str) -> String {
        format!("00000000000000000000000000000000-{rest}")
    }

    #[test]
    fn splits_common_names() {
        assert_eq!(package_base(&store("openssl-3.0.13")), "openssl");
        assert_eq!(package_base(&store("glibc-2.40-66")), "glibc");
        assert_eq!(
            package_base(&store("python3.12-requests-2.32.3")),
            "python3.12-requests"
        );
        assert_eq!(
            package_base(&store("nixos-system-x1carbon-26.11.20261027.31ecbac")),
            "nixos-system-x1carbon"
        );
        assert_eq!(package_base(&store("hello")), "hello");
    }

    #[test]
    fn prettifies_names() {
        let hash = "00000000000000000000000000000000";
        assert_eq!(
            pretty_name(&format!("/nix/store/{hash}-install-systemd-boot.sh")),
            "install systemd boot"
        );
        assert_eq!(
            pretty_name(&format!("/nix/store/{hash}-nixos-system-x1carbon-26.11.20261027.31ecbac")),
            "nixos system x1carbon 26.11.20261027.31ecbac"
        );
        assert_eq!(
            pretty_name(&format!("/nix/store/{hash}-glibc-2.40")),
            "glibc 2.40"
        );
    }

    #[test]
    fn extracts_versions() {
        assert_eq!(version_of(&store("openssl-3.0.13")), Some("3.0.13"));
        assert_eq!(version_of(&store("hello")), None);
    }
}
