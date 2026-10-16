//! Domain error types for Groshe Core.

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum CoreError {
    #[error("Item name cannot be empty")]
    EmptyItemName,

    #[error("List title cannot be empty")]
    EmptyListTitle,

    #[error("Invalid quantity: must be positive")]
    InvalidQuantity,

    #[error("Invalid fractional index position: {0}")]
    InvalidPosition(String),

    #[error("Invalid document key: {0}")]
    InvalidKey(String),

    #[error("Invalid UUID: {0}")]
    InvalidUuid(String),

    #[error("CBOR encoding error: {0}")]
    CborEncode(String),

    #[error("CBOR decoding error: {0}")]
    CborDecode(String),
}