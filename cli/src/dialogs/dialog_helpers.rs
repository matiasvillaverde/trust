use std::error::Error;
use std::fmt::Display;
use std::io::{Error as IoError, ErrorKind};

use dialoguer::{theme::ColorfulTheme, FuzzySelect};

pub fn require<T>(value: Option<T>, kind: ErrorKind, message: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| Box::new(IoError::new(kind, message)) as Box<dyn Error>)
}

pub fn select_from_list<T>(
    prompt: &str,
    items: &[T],
    empty_message: &str,
    cancel_message: &str,
) -> Result<T, Box<dyn Error>>
where
    T: Clone + Display,
{
    if items.is_empty() {
        return Err(Box::new(IoError::new(
            ErrorKind::NotFound,
            empty_message,
        )));
    }

    let selected = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(0)
        .interact_opt()
        .ok()
        .and_then(|selection| selection.and_then(|index| items.get(index)))
        .cloned();

    selected.ok_or_else(|| {
        Box::new(IoError::new(ErrorKind::InvalidInput, cancel_message)) as Box<dyn Error>
    })
}
