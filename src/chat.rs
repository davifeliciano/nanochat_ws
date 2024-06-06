use crate::verify_token;
use uuid::Uuid;

include!(concat!(env!("OUT_DIR"), "/chat.rs"));

impl Message {
    pub fn verify_token(&self) -> Result<Uuid, jsonwebtoken::errors::Error> {
        verify_token(&self.token)
    }
}
