// Finding 12. The text of fable/serdecheck/src/main.rs as recovered on #82, unchanged
// except for this header.
//
// The form and action types copied from sketch/core/addon.rs with their serde attributes unchanged.
// Only the placeholder types (Template, ExitCodes, ...) are filled in so that the file compiles.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormSchema { pub fields: Vec<Field> }

// Variant A: exactly as sketched. `kind` is a nested value.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field { pub key: String, pub label: String, pub help: Option<String>, #[serde(default)] pub required: bool, pub kind: FieldKind }

// Variant B: `kind` flattened, which is the only way the manifests in sketch/manifests parse
// (`key = "order_no"  label = "..."  kind = "text"  pattern = "..."` in one table).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlatField { pub key: String, pub label: String, pub help: Option<String>, #[serde(default)] pub required: bool, #[serde(flatten)] pub kind: FieldKind }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldKind {
	Text { pattern: Option<String>, max_len: Option<u32> },
	Integer { min: Option<i64>, max: Option<i64> },
	Bool,
	Choice { options: Vec<String> },
	Template,
	SqlText,
	Secret,
	SecretFile { max_bytes: u64 },
	ConnectionRef { connection: String },
	TaskSequenceRef,
	List { item: Vec<Field> },
}

fn try_kind(label: &str, toml_text: &str) {
	let nested: Result<Field, _> = toml::from_str(toml_text);
	let flat: Result<FlatField, _> = toml::from_str(toml_text);
	let short = |e: toml::de::Error| e.message().to_string();
	println!("{label}\n    input: {}\n    Field as sketched (nested kind): {}\n    Field with flattened kind:       {}",
		toml_text.replace('\n', "  "),
		match nested { Ok(f) => format!("ACCEPTED as {:?}", f.kind), Err(e) => format!("refused: {}", short(e)) },
		match flat { Ok(f) => format!("ACCEPTED as {:?}", f.kind), Err(e) => format!("refused: {}", short(e)) });
}

fn main() {
	// The shape the sketch's own manifest uses for a prompt field and for schemas/settings.json.
	try_kind("1. the sketch manifest's own field shape", "key = \"order_no\"\nlabel = \"Scan\"\nkind = \"text\"\npattern = \"^[0-9]+$\"");
	// An unknown key on a kind with fields: strict parsing should refuse it.
	try_kind("2. unknown key on a text field", "key = \"a\"\nlabel = \"A\"\nkind = \"text\"\nplaceholder = \"x\"");
	// An unknown key on kinds without fields. A later API level could add exactly these.
	try_kind("3. unknown key on a secret field", "key = \"pw\"\nlabel = \"Password\"\nkind = \"secret\"\nreveal_to_addon = true");
	try_kind("4. unknown key on a sql_text field", "key = \"statement\"\nlabel = \"Statement\"\nkind = \"sql_text\"\nallow_write = true");
	try_kind("5. unknown key on a bool field", "key = \"b\"\nlabel = \"B\"\nkind = \"bool\"\ndefault = true");
	// The same on the type exactly as sketched, in the JSON shape it does accept (a nested `kind` object).
	for doc in [
		r#"{"key":"pw","label":"Password","help":null,"required":false,"kind":{"kind":"secret","reveal_to_addon":true}}"#,
		r#"{"key":"q","label":"Statement","help":null,"required":false,"kind":{"kind":"sql_text","allow_write":true}}"#,
		r#"{"key":"t","label":"Text","help":null,"required":false,"kind":{"kind":"text","placeholder":"x"}}"#,
	] {
		let r: Result<Field, _> = serde_json::from_str(doc);
		println!("7. sketched Field from JSON {doc}\n    {}", match r { Ok(f) => format!("ACCEPTED as {:?}", f.kind), Err(e) => format!("refused: {e}") });
	}
	// An unknown kind.
	try_kind("6. unknown kind", "key = \"c\"\nlabel = \"C\"\nkind = \"color\"");
}
