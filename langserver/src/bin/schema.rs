fn main() {
    println!("{}", serde_json::to_string_pretty(&merman_langserver::schema()).unwrap());
}
