# Decode

`src-tauri/src/decode/` turns stored text into a value tree for value columns, inline trees and the WordPress field views,
and edits one leaf of it. It is core: the WordPress lens builds on it, never the other way round.

| File | Responsibility |
|---|---|
| `php.rs` | Lossless parser and serializer for PHP `serialize()` data; `PhpValue::get` and `as_text` for lookups; `looks_serialized` (WordPress `is_serialized`, plus `C:`); `is_php_float` |
| `value.rs` | Turns stored text (PHP or JSON) into a generic `ValueNode` tree. Serialized strings inside other values are decoded up to 3 levels deep |
| `edit.rs` | `edit_serialized`: replaces one leaf and re-serializes |
| `commands.rs` | The `decode_value` and `edit_serialized` Tauri commands |

## PHP serialize

`php.rs` round-trips exactly: `serialize(parse(x)) == x` for every value it accepts. The test suite checks
this, and `every_serialized_value_round_trips` (`src-tauri/src/wp/server_tests.rs`) checks it against every serialized value of a real database. The parser:

- keeps **strings as bytes**, because `s:N` counts bytes; invalid UTF-8 is preserved;
- keeps **floats as their source text**, accepting only PHP's float syntax (digits with optional sign,
  fraction and exponent, or `INF`, `-INF`, `NAN`);
- accepts **ints and bools only in the form `serialize()` writes** (`5`, `-5`, `b:0`, `b:1`; never `+5`,
  `05` or `-0`), so nothing is normalized on the way back;
- keeps **entry order**, including duplicate keys;
- **never runs or instantiates anything:** `O:` objects become a class name plus properties, while `C:`,
  references (counting from 1) and PHP 8.1 enums stay as opaque nodes;
- applies **limits:** nesting depth 128, preallocation capped at 1024; garbage input returns `ParseError`.

Decoding modes:
- **Strict** parsing fails at the first wrong byte length: a string's closing `"` and its terminator must sit
  exactly at the declared length.
- **Lenient** parsing repairs data that was re-encoded after it was serialized: when the declared length is
  wrong, the string ends at the closing `"` plus terminator nearest to the declared length. The whole input
  must still parse, so a wrong pick fails instead of producing a wrong tree.
- `value::decode` tries strict first. If only lenient succeeds, it returns the tree with a warning.

## Value tree

`ValueNode { kind, text, class, children, note }`; `kind` is one of `null`, `bool`, `int`, `float`, `string`,
`array`, `object`, `ref`, `enum`, `custom`. Children carry the display `key` and a `keyKind` (`int`,
`string`, or the property visibility `public`, `protected`, `private`); a private property also carries its
`declaringClass`, and a key that is not valid UTF-8 its bytes as `rawHex`. A string holding serialized data
gets one child `decoded` with the inner tree. JSON keeps key order; numbers are read as `i64`/`u64` or `f64` (`1.0` stays distinct from `1`, but `2.50`
shows as `2.5`, and larger integers lose digits).

## Editing

`edit::edit_serialized(raw, path, new)`:

1. The UI sends the raw value, the **index path** of the leaf (child positions as the tree lists them) and a
   typed `NewValue`. Indexes keep equal display keys apart (a protected and a public `bar`; int `1` and
   string `"1"`).
2. Index 0 of a string with a `decoded` child steps into the serialized data inside it.
3. PHP values are re-serialized, which recomputes every `s:N`; untouched parts, including whitespace around
   the value, stay byte-identical. JSON values are re-encoded: same key order, normalized spacing and escapes.
4. The result is only a string. It is written through the usual `queueChange` pipeline.

Only values that parse strictly can be edited. Refused edits:
- a float that is not PHP float syntax; Rust spellings like `inf` or `infinity` would be unreadable for
  `unserialize()`;
- any edit when an `r:`/`R:` reference sits at or after the edited value: references count values in
  serialization order, so replacing a subtree could make them point elsewhere.
