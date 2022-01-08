use serde::{Deserialize, Serialize};

extern crate serde_json;

#[derive(Serialize, Deserialize)]
pub struct PushResponse {
    #[serde(rename = "repository")]
    pub(crate) repository: Repository,
}

#[derive(Serialize, Deserialize)]
pub struct Author {
    #[serde(rename = "name")]
    pub(crate) name: String,

    #[serde(rename = "email")]
    pub(crate) email: String,

    #[serde(rename = "username")]
    pub(crate) username: String,
}

#[derive(Serialize, Deserialize)]
pub struct Pusher {
    #[serde(rename = "name")]
    pub(crate) name: String,

    #[serde(rename = "email")]
    pub(crate) email: String,
}

#[derive(Serialize, Deserialize)]
pub struct Repository {
    #[serde(rename = "id")]
    pub(crate) id: i64,

    #[serde(rename = "name")]
    pub(crate) name: String,

    #[serde(rename = "full_name")]
    pub(crate) full_name: String,

    #[serde(rename = "html_url")]
    pub(crate) url: String,
}