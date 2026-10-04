use {
    crate::spec::SpecDirection,
    std::fmt::{
        Display,
        Formatter,
        Result as FmtResult,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub path: String,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        let description: &'static str = match &self.kind {
            ErrorKind::UnknownStyle { .. } => "style doesn't exist",
            ErrorKind::NotTransverse { .. } => "converse and transverse directions do not cross",
            ErrorKind::ReservedAtomTypeId { .. } => "atom type id is reserved",
            ErrorKind::DuplicateAtomTypeIds { .. } => "duplicate atom type ids",
            ErrorKind::DuplicateAtomTypeIdsInGroup { .. } => "group id duplicates an atom type id",
            ErrorKind::DuplicateGroupId { .. } => "duplicate group ids",
            ErrorKind::TypeCircularReference { .. } => "type circular reference",
            ErrorKind::GroupChildDoesntExist { .. } => "group child doesn't exist",
            ErrorKind::EmptyGroup { .. } => "group covers no atom types",
            ErrorKind::AtomTypeDoesntExist { .. } => "specified candidate type doesn't exist",
            ErrorKind::RootBackIsKey => "the root back can't be a key/value pair",
            ErrorKind::KeyInvalidAtLocation => "key invalid at location",
            ErrorKind::KeyInvalidForGroupMember { .. } => "key invalid at location",
            ErrorKind::NonKeyInvalidAtLocation { .. } => "non-key invalid at location",
            ErrorKind::DuplicateBackId { .. } => "duplicate back ids in atom",
            ErrorKind::BackFieldWrongType { .. } => "back field is the wrong type",
            ErrorKind::ArrayMultipleAtoms => "array has multiple spliced sub-arrays",
            ErrorKind::RecordDiscardDuplicateKey { .. } => "duplicate record keys",
            ErrorKind::MissingBack { .. } => "missing back",
            ErrorKind::UnusedBackData { .. } => "unused data from back fields",
            ErrorKind::EmptyAlignmentBase => "alignment base name is empty",
            ErrorKind::NonexistentDefaultSelection { .. } => "field specified for default selection doesn't exist",
            ErrorKind::EmptyKeyBinding { .. } => "key binding has no strokes",
            ErrorKind::UnknownAction { .. } => "action doesn't exist",
            ErrorKind::AmbiguousKeyBinding { .. } => "key binding runs two actions on one cursor",
            ErrorKind::ShadowedKeyBinding { .. } => "key binding can never fire",
        };
        let values: Vec<(&'static str, String)> = match &self.kind {
            ErrorKind::UnknownStyle { style } => vec![("style", style.clone())],
            ErrorKind::NotTransverse { converse, transverse } => vec![
                ("converse", format!("{:?}", converse)),
                ("transverse", format!("{:?}", transverse)),
            ],
            ErrorKind::ReservedAtomTypeId { atom_type } |
            ErrorKind::DuplicateAtomTypeIds { atom_type } |
            ErrorKind::KeyInvalidForGroupMember { atom_type } |
            ErrorKind::NonKeyInvalidAtLocation { atom_type } => vec![
                ("atomType", atom_type.clone()),
            ],
            ErrorKind::DuplicateAtomTypeIdsInGroup { group } |
            ErrorKind::DuplicateGroupId { group } |
            ErrorKind::EmptyGroup { group } => vec![
                ("group", group.clone()),
            ],
            ErrorKind::TypeCircularReference { stack, member } => vec![
                ("stack", stack.join(" -> ")),
                ("member", member.clone()),
            ],
            ErrorKind::GroupChildDoesntExist { member } => vec![("member", member.clone())],
            ErrorKind::AtomTypeDoesntExist { candidate_type } => vec![("candidateType", candidate_type.clone())],
            ErrorKind::RootBackIsKey |
            ErrorKind::KeyInvalidAtLocation |
            ErrorKind::ArrayMultipleAtoms |
            ErrorKind::EmptyAlignmentBase => vec![
            ],
            ErrorKind::DuplicateBackId { id } => vec![("id", id.clone())],
            ErrorKind::BackFieldWrongType { field, found, expected } => vec![
                ("field", field.clone()),
                ("found", found.clone()),
                ("expected", expected.clone()),
            ],
            ErrorKind::RecordDiscardDuplicateKey { key } => vec![("key", key.clone())],
            ErrorKind::MissingBack { field } => vec![("field", field.clone())],
            ErrorKind::UnusedBackData { unused } => vec![("unused", unused.clone())],
            ErrorKind::NonexistentDefaultSelection { field } => vec![("defaultSelection", field.clone())],
            ErrorKind::EmptyKeyBinding { action } => vec![("action", action.clone())],
            ErrorKind::UnknownAction { action, known } => vec![
                ("action", action.clone()),
                ("known", known.join(", ")),
            ],
            ErrorKind::AmbiguousKeyBinding { binding, action, other } => vec![
                ("binding", binding.clone()),
                ("action", action.clone()),
                ("other", other.clone()),
            ],
            ErrorKind::ShadowedKeyBinding { binding, action, other_binding, other } => vec![
                ("binding", binding.clone()),
                ("action", action.clone()),
                ("otherBinding", other_binding.clone()),
                ("other", other.clone()),
            ],
        };
        writeln!(f, "{}", description)?;
        if !self.path.is_empty() {
            writeln!(f, "* at: {}", self.path)?;
        }
        for (key, value) in values {
            writeln!(f, "* {}: {}", key, value)?;
        }
        return Ok(());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    AmbiguousKeyBinding {
        binding: String,
        action: String,
        other: String,
    },
    ArrayMultipleAtoms,
    AtomTypeDoesntExist {
        candidate_type: String,
    },
    BackFieldWrongType {
        field: String,
        found: String,
        expected: String,
    },
    DuplicateAtomTypeIds {
        atom_type: String,
    },
    DuplicateAtomTypeIdsInGroup {
        group: String,
    },
    DuplicateBackId {
        id: String,
    },
    DuplicateGroupId {
        group: String,
    },
    EmptyAlignmentBase,
    EmptyGroup {
        group: String,
    },
    EmptyKeyBinding {
        action: String,
    },
    GroupChildDoesntExist {
        member: String,
    },
    KeyInvalidAtLocation,
    KeyInvalidForGroupMember {
        atom_type: String,
    },
    MissingBack {
        field: String,
    },
    NonexistentDefaultSelection {
        field: String,
    },
    NonKeyInvalidAtLocation {
        atom_type: String,
    },
    NotTransverse {
        converse: SpecDirection,
        transverse: SpecDirection,
    },
    RecordDiscardDuplicateKey {
        key: String,
    },
    ReservedAtomTypeId {
        atom_type: String,
    },
    RootBackIsKey,
    ShadowedKeyBinding {
        binding: String,
        action: String,
        other_binding: String,
        other: String,
    },
    TypeCircularReference {
        stack: Vec<String>,
        member: String,
    },
    UnknownAction {
        action: String,
        known: Vec<String>,
    },
    UnknownStyle {
        style: String,
    },
    UnusedBackData {
        unused: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MultiError(pub Vec<Error>);

impl MultiError {
    pub fn multi_error_add(&mut self, path: impl Into<String>, kind: ErrorKind) {
        self.0.push(Error {
            path: path.into(),
            kind: kind,
        });
    }

    pub fn multi_error_is_empty(&self) -> bool {
        return self.0.is_empty();
    }
}

impl Display for MultiError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        for error in &self.0 {
            write!(f, "{}", error)?;
        }
        return Ok(());
    }
}
