use super::edit::{self, NewValue};
use super::value::{self, DecodedValue};
use crate::error::AppResult;

#[tauri::command]
pub fn decode_value(text: String) -> Option<DecodedValue> {
    value::decode(&text)
}

#[tauri::command]
pub fn edit_serialized(raw: String, path: Vec<usize>, value: NewValue) -> AppResult<String> {
    edit::edit_serialized(&raw, &path, &value)
}
