/// Extract hashtags from content
pub fn extract_tags(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for word in content.split_whitespace() {
        if word.starts_with('#') && word.len() > 1 {
            let tag = word
                .trim_start_matches('#')
                .trim_end_matches(|c: char| !c.is_alphanumeric());
            if !tag.is_empty() {
                tags.push(tag.to_lowercase());
            }
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tags() {
        assert_eq!(extract_tags("hello #world"), vec!["world"]);
        assert_eq!(extract_tags("#one #two #three"), vec!["one", "three", "two"]);
        assert_eq!(extract_tags("no tags here"), Vec::<String>::new());
        assert_eq!(extract_tags("#Work meeting notes"), vec!["work"]);
        assert_eq!(extract_tags("# not a tag"), Vec::<String>::new());
    }
}
