use good_ormning::sqlite::{
    GenerateArgs,
    Version,
    VersionHandle,
    generate,
    schema::field::{
        field_bytes,
        field_i64,
        field_str,
    },
};

/// The tables of the JSON document history, in every version.
fn history(version: &VersionHandle) {
    let file = version.table("file");
    file.rowid_field(None);
    let path = file.field("path", field_str().build());
    file.field("position", field_i64().build());
    file.field("disk", field_str().build());
    file.field("disk_position", field_i64().opt().build());
    file.unique_index("file_path", &[&path]);
    let level = version.table("level");
    level.rowid_field(None);
    let level_file = level.field("file", field_i64().build());
    let seq = level.field("seq", field_i64().build());
    level.field("steps", field_str().build());
    level.field("select_before", field_str().opt().build());
    level.field("select_after", field_str().opt().build());
    level.unique_index("level_file_seq", &[&level_file, &seq]);
    return;
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let v0 = Version::new();
    history(&v0);
    // Version 1 adds the editors' documents: a file held as bytes whose undo levels are blobs only
    // the editor that wrote them (tagged by `editor`) can apply.
    let v1 = Version::new();
    history(&v1);
    let doc = v1.table("doc");
    doc.rowid_field(None);
    let doc_path = doc.field("path", field_str().build());
    doc.field("position", field_i64().build());
    doc.field("contents", field_bytes().build());
    doc.unique_index("doc_path", &[&doc_path]);
    let doc_level = v1.table("doc_level");
    doc_level.rowid_field(None);
    let doc_level_doc = doc_level.field("doc", field_i64().build());
    let doc_seq = doc_level.field("seq", field_i64().build());
    doc_level.field("editor", field_str().build());
    doc_level.field("data", field_bytes().build());
    doc_level.unique_index("doc_level_doc_seq", &[&doc_level_doc, &doc_seq]);
    generate(GenerateArgs {
        versions: vec![(0usize, v0.build()), (1usize, v1.build())],
        ..Default::default()
    }).unwrap();
}
