const fs = require('fs');
const NL = String.fromCharCode(10);
let t = fs.readFileSync('crates/lekalo-core/src/openapi/schema.rs', 'utf8');
const anchor = '''    #[test]
    fn optional_30_uses_the_nullable_sibling() {'''.split(String.fromCharCode(10)).join(NL);
const added = '''    #[test]
    fn nested_optional_never_duplicates_null() {
        // `optional<optional<string>>` widens to exactly one `"null"`
        // member: the meta-schema requires unique type-array items.
        let project = empty_project();
        let mapper = SchemaMapper::new(&project, DocumentVersion::V3_1);
        let once = mapper.optional(json!({ "type": ["string", "null"] }));
        assert_eq!(once, json!({ "type": ["string", "null"] }));
        let inner = mapper.optional(json!({ "type": "string" }));
        let twice = mapper.optional(inner);
        assert_eq!(twice, json!({ "type": ["string", "null"] }));
    }

''' + anchor;
if (!t.includes(anchor)) { console.log('ANCHOR NOT FOUND'); process.exit(1); }
t = t.split(anchor).join(added);
fs.writeFileSync('crates/lekalo-core/src/openapi/schema.rs', t);
console.log('schema test added');
