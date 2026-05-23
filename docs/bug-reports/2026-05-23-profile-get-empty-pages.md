# Bug Report: profile_get และ wiki_list คืนค่า pages array ว่างเมื่อกรองด้วย type filter

- **Date**: 2026-05-23
- **Status**: Fixed
- **Severity**: High
- **Affected version**: 0.4.3
- **Fixed version**: 0.4.4+

## อาการ

เรียก `profile_get(section="rules")` หรือ `wiki_list(type="profile")` ผ่าน MCP ได้ `pages: []` แม้ข้อมูลมีอยู่ใน index (ยืนยันด้วย `wiki_search` และ `wiki_stats`)

## ระบบที่ได้รับผลกระทบ

- `profile_get` — ทุก section
- `wiki_list` — เมื่อมี type filter
- `semantic_search` — เมื่อมี type filter
- ทุก function ที่เรียก `ops::search::list()` ด้วย `type_filter`

## ระบบที่ไม่มีปัญหา

- `wiki_search` — ใช้ BM25 search บน `body` field
- `wiki_stats` — ใช้ facet counting ไม่ใช้ TermQuery
- `wiki_content_read` — อ่านโดย slug ตรงๆ

## Root Cause Analysis

ปัญหาเกิดจาก **schema field classification** ที่ทำให้ `type` field ตกเป็น **Text** field แทน **Keyword** field:

### 1. Schema Loading Order

ไฟล์ schema ใน `schemas/` ถูกโหลดตามลำดับตัวอักษร:

```
base.json → concept.json → doc.json → ... → profile.json → ...
```

`base.json` โหลดก่อน `profile.json` เสมอ

### 2. Field Classification ใน base.json

`base.json` นิยาม field `type` ว่า:

```json
"type": {
  "type": "string",
  "description": "Page type from registry"
}
```

ไม่มี `enum` หรือ `const` → `classify_field()` จัดเป็น `FieldClass::Text`

### 3. Field ซ้ำถูก Skip

`profile.json` นิยาม field `type` ว่า:

```json
"type": {
  "type": "string",
  "const": "profile",
  "description": "Page type"
}
```

มี `const` → จัดเป็น `FieldClass::Keyword` ได้ แต่ `type` มีอยู่แล้วใน `seen_fields` set จึงถูก **skip**:

```rust
// space_builder.rs
if seen_fields.contains(field_name) {
    continue;  // ← type จาก profile.json ถูกข้าม
}
```

### 4. Tokenizer Stemming ทำให้ TermQuery ไม่ match

`type` field ถูกสร้างเป็น Text field ด้วย tokenizer `en_stem` (default):

| ขั้นตอน | ค่า | หมายเหตุ |
|---|---|---|
| Frontmatter | `"profile"` | ค่าดิบใน Markdown |
| Index time | `"profil"` | `en_stem` (Porter stemmer) stem ลบ suffix `e` |
| Query time | `"profile"` | `Term::from_field_text()` ส่งค่าดิบ ไม่ผ่าน tokenizer |
| **ผลลัพธ์** | `"profil" ≠ "profile"` | **ไม่ match!** |

Query side:

```rust
// search.rs — query ใช้ค่าดิบ ไม่ผ่าน stemmer
TermQuery::new(
    Term::from_field_text(f_type, "profile"),
    IndexRecordOption::Basic,
)
```

Index side:

```rust
// index_manager.rs — indexing ผ่าน tokenizer
doc.add_text(field_handle, "profile");
// "profile" → en_stem tokenizer → "profil" ใน index
```

### 5. status field ไม่มีปัญหา

`status` field ใน `base.json` มี `enum: ["active", "draft", "stub", "generated"]` → จัดเป็น Keyword ตั้งแต่แรก → `TermQuery("active")` match ได้ถูกต้อง

## แผนภาพสาเหตุ

```
base.json (alphabetically first)
  └── "type": { "type": "string" }  ← ไม่มี enum/const
        │
        ▼
  classify_field() → FieldClass::Text
        │
        ▼
  add_text("type") → TextField with en_stem tokenizer
        │
        ▼
profile.json (loaded later)
  └── "type": { "const": "profile" }
        │
        ▼
  seen_fields.contains("type") → true → SKIPPED
        │
        ▼
  type field = Text (tokenized, stemmed)
        │
        ├── Index: "profile" → stemmer → "profil"
        │
        └── Query: TermQuery("profile") → raw "profile"
                                        │
                                        ▼
                                  "profil" ≠ "profile" → pages: []
```

## Fix

เพิ่ม `type` เป็น **fixed keyword field** (เหมือน `slug`, `uri`) เพราะ `type` เป็น categorical field ที่ใช้ exact-match filtering เสมอ

### Files changed

**`src/index_schema.rs`** — `add_fixed_fields()`:

```rust
self.add_keyword("uri");
self.add_keyword("type");  // ← เพิ่ม
self.add_text("body");
```

**`src/index_schema.rs`** — `seen` set:

```rust
for name in &["slug", "uri", "type", "body", "body_links"] {
```

**`src/space_builder.rs`** — `seen_fields` (2 ที่):

```rust
let mut seen_fields: HashSet<String> = ["slug", "uri", "type", "body", "body_links"]
```

## ผลกระทบจาก Fix

- `type` field เป็น Keyword (STRING | STORED | FAST) → `TermQuery` exact match ได้ถูกต้อง
- Schema files ที่นิยาม `type` จะถูก skip (เหมือนเดิม) แต่ใช้ keyword definition จาก fixed field แทน
- **ต้อง rebuild index** (`llm-wiki index rebuild`) หลัง deploy binary ใหม่ เพราะ field type เปลี่ยน

## Deploy Steps

หลัง deploy binary ใหม่ต้อง rebuild index:

```bash
llm-wiki index rebuild --wiki brain
sudo systemctl restart brain-mcp
```

## Verification

เทสหลัง deploy:

```
profile_get(section="rules") → pages ไม่ว่าง
wiki_list(type="profile")    → pages ไม่ว่าง
```

Automated regression coverage:

```bash
cargo test --test index_schema
```

The regression test asserts that `type` exists in the embedded schema and is
classified as a keyword field, so future schema changes cannot silently turn
type filters back into stemmed text queries.
