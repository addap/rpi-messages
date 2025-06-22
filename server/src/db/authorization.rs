use std::fmt;

use serde::{Deserialize, Serialize};
use teloxide::types::{User, UserId};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AuthReplyChoice {
    Accept,
    Deny,
}

impl fmt::Display for AuthReplyChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthReplyChoice::Accept => f.write_str("Accept"),
            AuthReplyChoice::Deny => f.write_str("Deny"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthReply {
    auth_request_id: Uuid,
    choice: AuthReplyChoice,
}

impl AuthReply {
    pub fn new(auth_request_id: Uuid, choice: AuthReplyChoice) -> Self {
        Self {
            auth_request_id,
            choice,
        }
    }

    pub fn id(&self) -> Uuid {
        self.auth_request_id
    }

    pub fn choice(&self) -> AuthReplyChoice {
        self.choice
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    id: Uuid,
    user_id: UserId,
    user_name: String,
}

impl AuthRequest {
    pub fn new(user: &User) -> Self {
        Self {
            id: Uuid::now_v7(),
            user_id: user.id,
            user_name: user.full_name(),
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    pub fn user_name(&self) -> &str {
        &self.user_name
    }
}
