use std::path::{Path, PathBuf};

/// Expands `${VAR}` and `$VAR` using the process environment.
/// Unknown variables are left untouched so the failure is visible downstream.
pub fn expand_env_vars(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();

    while let Some((index, current)) = chars.next() {
        if current != '$' {
            output.push(current);
            continue;
        }

        let rest = &input[index + 1..];
        if let Some(stripped) = rest.strip_prefix('{') {
            if let Some(end) = stripped.find('}') {
                let name = &stripped[..end];
                match std::env::var(name) {
                    Ok(value) => output.push_str(&value),
                    Err(_) => output.push_str(&input[index..index + name.len() + 3]),
                }
                for _ in 0..name.len() + 2 {
                    chars.next();
                }
                continue;
            }
            output.push(current);
            continue;
        }

        let name_len = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        if name_len == 0 {
            output.push(current);
            continue;
        }
        let name = &rest[..name_len];
        match std::env::var(name) {
            Ok(value) => output.push_str(&value),
            Err(_) => {
                output.push(current);
                output.push_str(name);
            }
        }
        for _ in 0..name_len {
            chars.next();
        }
    }

    output
}

/// Expands a manifest path: environment variables first, then a leading `~`.
pub fn expand_path(input: &str, home: &Path) -> PathBuf {
    let expanded = expand_env_vars(input);
    if expanded == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = expanded.strip_prefix("~/") {
        return home.join(rest);
    }
    #[cfg(target_os = "windows")]
    if let Some(rest) = expanded.strip_prefix("~\\") {
        return home.join(rest);
    }
    PathBuf::from(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde_with_provided_home() {
        let home = Path::new("/home/tester");
        assert_eq!(
            expand_path("~/.config/app.toml", home),
            PathBuf::from("/home/tester/.config/app.toml")
        );
        assert_eq!(expand_path("~", home), PathBuf::from("/home/tester"));
        assert_eq!(expand_path("/etc/hosts", home), PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn expands_known_env_vars_and_keeps_unknown() {
        std::env::set_var("SWISS_TEST_EXPAND", "value");
        assert_eq!(expand_env_vars("a/${SWISS_TEST_EXPAND}/b"), "a/value/b");
        assert_eq!(expand_env_vars("a/$SWISS_TEST_EXPAND/b"), "a/value/b");
        assert_eq!(
            expand_env_vars("a/${SWISS_TEST_MISSING_VAR}/b"),
            "a/${SWISS_TEST_MISSING_VAR}/b"
        );
        assert_eq!(expand_env_vars("price: 5$"), "price: 5$");
    }
}
