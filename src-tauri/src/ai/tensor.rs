use serde::{Deserialize, Serialize};

/// Supported tensor data types for AI models.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TensorDataType {
    Float32,
    Float16,
    Int32,
    Int64,
    Uint8,
    Int8,
}

/// Dimension specification supporting both fixed and dynamic dimension shapes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Dimension {
    Fixed(u64),
    Dynamic(String),
}

impl Dimension {
    pub fn fixed(val: u64) -> Self {
        Self::Fixed(val)
    }

    pub fn dynamic(name: impl Into<String>) -> Self {
        Self::Dynamic(name.into())
    }
}

/// Specification of a tensor input or output for an AI model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TensorSpec {
    pub name: String,
    pub data_type: TensorDataType,
    pub shape: Vec<Dimension>,
}

impl TensorSpec {
    pub fn new(name: impl Into<String>, data_type: TensorDataType, shape: Vec<Dimension>) -> Self {
        Self {
            name: name.into(),
            data_type,
            shape,
        }
    }
}
