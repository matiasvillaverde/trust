use std::error::Error;
use std::fmt::Display;
use std::io::{Error as IoError, ErrorKind};

use crate::dialogs::DialogIo;

pub fn require<T>(value: Option<T>, kind: ErrorKind, message: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| Box::new(IoError::new(kind, message)) as Box<dyn Error>)
}

pub fn select_from_list<T>(
    io: &mut dyn DialogIo,
    prompt: &str,
    items: &[T],
    empty_message: &str,
    cancel_message: &str,
) -> Result<T, Box<dyn Error>>
where
    T: Clone + Display,
{
    if items.is_empty() {
        return Err(Box::new(IoError::new(ErrorKind::NotFound, empty_message)));
    }

    let labels = items
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    let selected = io
        .select_index(prompt, &labels, 0)
        .ok()
        .and_then(|selection| selection.and_then(|index| items.get(index)))
        .cloned();

    selected.ok_or_else(|| {
        Box::new(IoError::new(ErrorKind::InvalidInput, cancel_message)) as Box<dyn Error>
    })
}

#[cfg(test)]
mod tests {
    use super::select_from_list;
    use crate::dialogs::DialogIo;
    use std::collections::VecDeque;
    use std::io::{Error as IoError, ErrorKind};

    #[derive(Default)]
    struct ScriptedIo {
        selections: VecDeque<Result<Option<usize>, IoError>>,
    }

    impl DialogIo for ScriptedIo {
        fn select_index(
            &mut self,
            _prompt: &str,
            _labels: &[String],
            _default: usize,
        ) -> Result<Option<usize>, IoError> {
            self.selections.pop_front().unwrap_or(Ok(None))
        }

        fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, IoError> {
            Ok(false)
        }
    }

    #[test]
    fn select_from_list_returns_selected_item() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(1)));
        let items = ["one", "two", "three"];

        let selected = select_from_list(&mut io, "Pick", &items, "empty", "cancel")
            .expect("selection should succeed");

        assert_eq!(selected, "two");
    }

    #[test]
    fn select_from_list_errors_when_empty() {
        let mut io = ScriptedIo::default();
        let items: [String; 0] = [];
        let error =
            select_from_list(&mut io, "Pick", &items, "empty list", "cancel").expect_err("empty");
        assert_eq!(
            error.downcast_ref::<IoError>().map(IoError::kind),
            Some(ErrorKind::NotFound)
        );
    }

    #[test]
    fn select_from_list_errors_on_cancel() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(None));
        let items = ["one", "two"];

        let error = select_from_list(&mut io, "Pick", &items, "empty", "cancelled")
            .expect_err("cancel should error");
        assert_eq!(
            error.downcast_ref::<IoError>().map(IoError::kind),
            Some(ErrorKind::InvalidInput)
        );
    }

    #[test]
    fn select_from_list_errors_on_io_failure() {
        let mut io = ScriptedIo::default();
        io.selections
            .push_back(Err(IoError::other("terminal unavailable")));
        let items = ["one", "two"];

        let error = select_from_list(&mut io, "Pick", &items, "empty", "cancelled")
            .expect_err("io failure should error");
        assert_eq!(
            error.downcast_ref::<IoError>().map(IoError::kind),
            Some(ErrorKind::InvalidInput)
        );
    }
}
