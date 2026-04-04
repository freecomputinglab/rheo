pub fn is_relative_typ_link(url: &str) -> bool {
    if !url.ends_with(".typ") {
        return false;
    }

    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with("//")
        || url.starts_with('#')
    {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_relative_typ_link() {
        assert!(is_relative_typ_link("./chapter1.typ"));
        assert!(is_relative_typ_link("../other.typ"));
        assert!(is_relative_typ_link("file.typ"));

        assert!(!is_relative_typ_link("https://example.com/file.typ"));
        assert!(!is_relative_typ_link("http://example.com"));
        assert!(!is_relative_typ_link("mailto:test@example.com"));
        assert!(!is_relative_typ_link("#anchor"));
        assert!(!is_relative_typ_link("./file.md"));
    }
}
