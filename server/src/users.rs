use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

// todo private supertrait to prevent other impls.
pub trait Auth {}

#[derive(Debug)]
pub struct Unauthenticated;
#[derive(Debug)]
pub struct Authenticated;

#[derive(Debug, Serialize, Deserialize)]
pub struct User<T> {
    #[serde(skip)]
    _auth: PhantomData<T>,
    raw: RawUser,
}

impl User<Unauthenticated> {
    pub fn new_telegram(id: teloxide::types::UserId) -> Self {
        Self {
            _auth: PhantomData,
            raw: RawUser::Telegram { id },
        }
    }

    pub fn authenticate(self) -> User<Authenticated> {
        User {
            _auth: PhantomData,
            raw: self.raw,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum RawUser {
    Telegram { id: teloxide::types::UserId },
}
