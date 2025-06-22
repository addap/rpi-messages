use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// Trait to classify authentication states.
/// Sealed supertrait not necessary because this is a binary crate anyways.
pub trait Auth {}

#[derive(Debug, Clone, Copy)]
pub struct Unauthorized;
#[derive(Debug, Clone, Copy)]
pub struct Authorized;

impl Auth for Unauthorized {}
impl Auth for Authorized {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
// #[serde(transparent)]
pub struct User<T> {
    #[serde(skip)]
    _auth: PhantomData<T>,
    raw: RawUser,
}

impl<T: Auth> User<T> {
    pub fn raw(&self) -> RawUser {
        self.raw
    }
}

impl User<Unauthorized> {
    pub fn new_telegram(id: teloxide::types::UserId) -> Self {
        Self {
            _auth: PhantomData,
            raw: RawUser::Telegram { id },
        }
    }

    pub fn authorize(self) -> User<Authorized> {
        User {
            _auth: PhantomData,
            raw: self.raw,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(crate) enum RawUser {
    Telegram { id: teloxide::types::UserId },
}
