pub fn init_table(name: &str, cols: &str) -> String {
    format!(
        "create table if not exists {name} ({cols})")
}
