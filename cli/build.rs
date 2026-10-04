use good_ormning::sqlite::{
    GenerateArgs,
    Version,
    generate,
    schema::field::{
        field_i64,
        field_str,
    },
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let latest = Version::new();
    let file = latest.table("file");
    file.rowid_field(None);
    let path = file.field("path", field_str().build());
    file.field("position", field_i64().build());
    file.field("disk", field_str().build());
    file.field("disk_position", field_i64().opt().build());
    file.unique_index("file_path", &[&path]);
    let level = latest.table("level");
    level.rowid_field(None);
    let level_file = level.field("file", field_i64().build());
    let seq = level.field("seq", field_i64().build());
    level.field("steps", field_str().build());
    level.field("select_before", field_str().opt().build());
    level.field("select_after", field_str().opt().build());
    level.unique_index("level_file_seq", &[&level_file, &seq]);
    generate(GenerateArgs {
        versions: vec![(0usize, latest.build())],
        ..Default::default()
    }).unwrap();
}
