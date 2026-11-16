fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&labyrinth_api::openapi::document())
            .expect("serializable contract")
    );
}
