use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub enum NewMessage {
    Text { text: String },
}
