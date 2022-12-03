//! Shared types relating to emails.

use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};

/// Email address.
#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, Clone)]
#[serde(transparent)]
pub struct Address(lettre::Address);

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for Address {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<'a> TryFrom<&mail_parser::Addr<'a>> for Address {
    type Error = eyre::Error;
    fn try_from(address: &mail_parser::Addr<'a>) -> Result<Self, Self::Error> {
        if let Some(address) = &address.address {
            Ok(Self(address.parse()?))
        } else {
            Err(eyre::eyre!(
                "Addr {:?} does not contain an address",
                address
            ))
        }
    }
}

impl Into<lettre::Address> for Address {
    fn into(self) -> lettre::Address {
        self.0
    }
}

impl Into<lettre::message::Mailbox> for Address {
    fn into(self) -> lettre::message::Mailbox {
        lettre::message::Mailbox {
            name: None,
            email: self.0,
        }
    }
}

/// An email account address/username e.g. `my.email@example.com`, or `Name <name@example.com>`
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Account(lettre::message::Mailbox);

impl Account {
    /// Obtain `&str` to the email address portion of the account. e.g. `hello@example.com`.
    #[must_use]
    pub fn email_str(&self) -> &str {
        self.0.email.as_ref()
    }

    /// Obtain the email [`Address`] portion of the account. e.g. `hello@example.com`.
    #[must_use]
    pub fn email(&self) -> Address {
        Address(self.0.email.clone())
    }
}

impl FromStr for Account {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Account(s.parse()?))
    }
}

impl<'a> TryFrom<&mail_parser::Addr<'a>> for Account {
    type Error = eyre::Error;
    fn try_from(address: &mail_parser::Addr<'a>) -> Result<Self, Self::Error> {
        let email: lettre::Address = if let Some(address) = &address.address {
            address.parse()?
        } else {
            return Err(eyre::eyre!(
                "Addr {:?} does not contain an address",
                address
            ));
        };

        let name: Option<String> = address.name.as_ref().map(ToString::to_string);

        Ok(Self(lettre::message::Mailbox { name, email }))
    }
}
impl Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Into<lettre::message::Mailbox> for Account {
    fn into(self) -> lettre::message::Mailbox {
        self.0
    }
}
