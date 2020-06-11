pub fn clean_name(name: &str) -> String {
    name.to_lowercase()
        .replace(' ', "_")
        .replace('\'', "")
        .replace('"', "")
}
