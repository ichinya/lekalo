use lekalo_core::storage_projection::{canonical, project, Namespace, StorageProjectionAttachment};

fn main() {
    let root = std::env::args().nth(1).expect("repo root");
    let text = std::fs::read_to_string(format!(
        "{root}/tests/fixtures/storage-projection/valid/planner-storage.json"
    ))
    .expect("golden");
    let value: serde_json::Value = serde_json::from_str(text.trim()).expect("json");
    let attachment = StorageProjectionAttachment::from_value(&value).expect("attachment");
    for (namespace, name) in [
        (Namespace::Postgres, "postgres"),
        (Namespace::Laravel, "laravel"),
        (Namespace::Mysql, "mysql"),
        (Namespace::Mariadb, "mariadb"),
    ] {
        let derived = project(&attachment, namespace).expect("derives");
        let bytes = canonical::derived_bytes(&derived).expect("bytes");
        std::fs::write(
            format!("{root}/tests/fixtures/storage-projection/derived/{name}.json"),
            format!(
                "{bytes}
"
            ),
        )
        .expect("write");
        println!("wrote {name}.json");
    }
}
