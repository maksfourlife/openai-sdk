use chrono::{DateTime, Utc};
use derive_more::From;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::define_ids;

#[cfg(feature = "responses-streaming")]
pub mod streaming;

define_ids!(ResponseId);

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Response {
    /// Whether to run the model response in the background. [Learn more.](https://platform.openai.com/docs/guides/background)
    pub background: Option<bool>,
    /// Unix timestamp (in seconds) of when this Response was created.
    #[serde_as(as = "serde_with::TimestampSeconds")]
    pub created_at: DateTime<Utc>,
    /// Unique identifier for this Response.
    pub id: ResponseId,
}

#[derive(Debug, Clone, From, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ResponseInput {
    /// A text input to the model, equivalent to a text input with the `user` role.
    Text(String),
    /// A list of one or many input items to the model, containing different content types.
    ItemList(Vec<ResponseInputItem>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ResponseInputItem {}
