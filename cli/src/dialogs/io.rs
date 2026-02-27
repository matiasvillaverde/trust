#[cfg(not(test))]
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect};
use std::io::Error as IoError;
use std::io::ErrorKind;

pub trait DialogIo {
    fn select_index(
        &mut self,
        prompt: &str,
        labels: &[String],
        default: usize,
    ) -> Result<Option<usize>, IoError>;

    fn confirm(&mut self, prompt: &str, default: bool) -> Result<bool, IoError>;

    fn input_text(&mut self, _prompt: &str, _allow_empty: bool) -> Result<String, IoError> {
        Err(IoError::new(
            ErrorKind::Unsupported,
            "input_text not implemented",
        ))
    }
}

pub trait DialogBackend {
    fn select_index(
        &mut self,
        prompt: &str,
        labels: &[String],
        default: usize,
    ) -> Result<Option<usize>, dialoguer::Error>;

    fn confirm(&mut self, prompt: &str, default: bool) -> Result<bool, dialoguer::Error>;

    fn input_text(&mut self, prompt: &str, allow_empty: bool) -> Result<String, dialoguer::Error>;
}

#[derive(Default)]
pub struct DialoguerBackend;

#[cfg(test)]
mod scripted_backend {
    use std::collections::VecDeque;
    use std::io::Error as IoError;
    use std::sync::{Mutex, OnceLock};

    #[derive(Default)]
    pub struct ScriptedState {
        pub selects: VecDeque<Result<Option<usize>, IoError>>,
        pub confirms: VecDeque<Result<bool, IoError>>,
        pub inputs: VecDeque<Result<String, IoError>>,
    }

    fn state() -> &'static Mutex<ScriptedState> {
        static STATE: OnceLock<Mutex<ScriptedState>> = OnceLock::new();
        STATE.get_or_init(|| Mutex::new(ScriptedState::default()))
    }

    pub(crate) fn push_select(result: Result<Option<usize>, IoError>) {
        let mut guard = state().lock().expect("scripted state lock");
        guard.selects.push_back(result);
    }

    #[allow(dead_code)]
    pub(crate) fn push_confirm(result: Result<bool, IoError>) {
        let mut guard = state().lock().expect("scripted state lock");
        guard.confirms.push_back(result);
    }

    pub(crate) fn push_input(result: Result<String, IoError>) {
        let mut guard = state().lock().expect("scripted state lock");
        guard.inputs.push_back(result);
    }

    pub(crate) fn reset() {
        let mut guard = state().lock().expect("scripted state lock");
        *guard = ScriptedState::default();
    }

    pub(crate) fn next_select() -> Result<Option<usize>, IoError> {
        let mut guard = state().lock().expect("scripted state lock");
        guard
            .selects
            .pop_front()
            .unwrap_or_else(|| Err(IoError::other("no scripted selection response")))
    }

    pub(crate) fn next_confirm() -> Result<bool, IoError> {
        let mut guard = state().lock().expect("scripted state lock");
        guard
            .confirms
            .pop_front()
            .unwrap_or_else(|| Err(IoError::other("no scripted confirm response")))
    }

    pub(crate) fn next_input() -> Result<String, IoError> {
        let mut guard = state().lock().expect("scripted state lock");
        guard
            .inputs
            .pop_front()
            .unwrap_or_else(|| Err(IoError::other("no scripted input response")))
    }
}

impl DialogBackend for DialoguerBackend {
    #[cfg(not(test))]
    fn select_index(
        &mut self,
        prompt: &str,
        labels: &[String],
        default: usize,
    ) -> Result<Option<usize>, dialoguer::Error> {
        FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .items(labels)
            .default(default)
            .interact_opt()
    }

    #[cfg(test)]
    fn select_index(
        &mut self,
        _prompt: &str,
        _labels: &[String],
        _default: usize,
    ) -> Result<Option<usize>, dialoguer::Error> {
        scripted_backend::next_select().map_err(dialoguer::Error::IO)
    }

    #[cfg(not(test))]
    fn confirm(&mut self, prompt: &str, default: bool) -> Result<bool, dialoguer::Error> {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .default(default)
            .interact()
    }

    #[cfg(test)]
    fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, dialoguer::Error> {
        scripted_backend::next_confirm().map_err(dialoguer::Error::IO)
    }

    #[cfg(not(test))]
    fn input_text(&mut self, prompt: &str, allow_empty: bool) -> Result<String, dialoguer::Error> {
        dialoguer::Input::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .allow_empty(allow_empty)
            .interact_text()
    }

    #[cfg(test)]
    fn input_text(
        &mut self,
        _prompt: &str,
        _allow_empty: bool,
    ) -> Result<String, dialoguer::Error> {
        scripted_backend::next_input().map_err(dialoguer::Error::IO)
    }
}

pub struct ConsoleDialogIo<B = DialoguerBackend> {
    backend: B,
}

impl Default for ConsoleDialogIo<DialoguerBackend> {
    fn default() -> Self {
        Self {
            backend: DialoguerBackend,
        }
    }
}

impl<B: DialogBackend> DialogIo for ConsoleDialogIo<B> {
    fn select_index(
        &mut self,
        prompt: &str,
        labels: &[String],
        default: usize,
    ) -> Result<Option<usize>, IoError> {
        self.backend
            .select_index(prompt, labels, default)
            .map_err(Self::to_io_error)
    }

    fn confirm(&mut self, prompt: &str, default: bool) -> Result<bool, IoError> {
        self.backend
            .confirm(prompt, default)
            .map_err(Self::to_io_error)
    }

    fn input_text(&mut self, prompt: &str, allow_empty: bool) -> Result<String, IoError> {
        self.backend
            .input_text(prompt, allow_empty)
            .map_err(Self::to_io_error)
    }
}

impl<B> ConsoleDialogIo<B> {
    #[cfg(test)]
    fn with_backend(backend: B) -> Self {
        Self { backend }
    }

    fn to_io_error(error: dialoguer::Error) -> IoError {
        IoError::other(error.to_string())
    }
}

#[cfg(test)]
pub(crate) fn scripted_push_select(result: Result<Option<usize>, IoError>) {
    scripted_backend::push_select(result);
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn scripted_push_confirm(result: Result<bool, IoError>) {
    scripted_backend::push_confirm(result);
}

#[cfg(test)]
pub(crate) fn scripted_push_input(result: Result<String, IoError>) {
    scripted_backend::push_input(result);
}

#[cfg(test)]
pub(crate) fn scripted_reset() {
    scripted_backend::reset();
}

#[cfg(test)]
mod tests {
    use super::{
        scripted_push_confirm, scripted_push_input, scripted_push_select, scripted_reset,
        ConsoleDialogIo, DialogBackend, DialogIo, DialoguerBackend,
    };
    use std::io::ErrorKind;

    struct StubBackend {
        next_select: Result<Option<usize>, dialoguer::Error>,
        next_confirm: Result<bool, dialoguer::Error>,
    }

    impl DialogBackend for StubBackend {
        fn select_index(
            &mut self,
            _prompt: &str,
            _labels: &[String],
            _default: usize,
        ) -> Result<Option<usize>, dialoguer::Error> {
            std::mem::replace(&mut self.next_select, Ok(None))
        }

        fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, dialoguer::Error> {
            std::mem::replace(&mut self.next_confirm, Ok(false))
        }

        fn input_text(
            &mut self,
            _prompt: &str,
            _allow_empty: bool,
        ) -> Result<String, dialoguer::Error> {
            Ok("stub".to_string())
        }
    }

    #[test]
    fn to_io_error_maps_to_other_kind_and_preserves_message() {
        let source =
            dialoguer::Error::IO(std::io::Error::new(ErrorKind::PermissionDenied, "blocked"));

        let mapped = ConsoleDialogIo::<StubBackend>::to_io_error(source);
        assert_eq!(mapped.kind(), ErrorKind::Other);
        assert!(mapped.to_string().contains("blocked"));
    }

    #[test]
    fn select_index_delegates_to_backend_and_returns_selected_value() {
        let mut io = ConsoleDialogIo::with_backend(StubBackend {
            next_select: Ok(Some(2)),
            next_confirm: Ok(true),
        });
        let labels = vec!["a".to_string(), "b".to_string(), "c".to_string()];

        let result = io.select_index("Pick one", &labels, 1).unwrap();
        assert_eq!(result, Some(2));
    }

    #[test]
    fn select_index_maps_backend_error_to_io_error() {
        let mut io = ConsoleDialogIo::with_backend(StubBackend {
            next_select: Err(dialoguer::Error::IO(std::io::Error::new(
                ErrorKind::PermissionDenied,
                "blocked select",
            ))),
            next_confirm: Ok(true),
        });
        let labels = vec!["a".to_string()];

        let error = io
            .select_index("Pick one", &labels, 0)
            .expect_err("error should be mapped");
        assert_eq!(error.kind(), ErrorKind::Other);
        assert!(error.to_string().contains("blocked select"));
    }

    #[test]
    fn confirm_delegates_to_backend_and_returns_value() {
        let mut io = ConsoleDialogIo::with_backend(StubBackend {
            next_select: Ok(None),
            next_confirm: Ok(true),
        });

        let confirmed = io.confirm("Continue?", false).unwrap();
        assert!(confirmed);
    }

    #[test]
    fn confirm_maps_backend_error_to_io_error() {
        let mut io = ConsoleDialogIo::with_backend(StubBackend {
            next_select: Ok(None),
            next_confirm: Err(dialoguer::Error::IO(std::io::Error::new(
                ErrorKind::NotConnected,
                "broken terminal",
            ))),
        });

        let error = io
            .confirm("Continue?", false)
            .expect_err("error should be mapped");
        assert_eq!(error.kind(), ErrorKind::Other);
        assert!(error.to_string().contains("broken terminal"));
    }

    #[test]
    fn input_text_delegates_to_backend() {
        struct InputBackend;
        impl DialogBackend for InputBackend {
            fn select_index(
                &mut self,
                _prompt: &str,
                _labels: &[String],
                _default: usize,
            ) -> Result<Option<usize>, dialoguer::Error> {
                Ok(Some(0))
            }
            fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, dialoguer::Error> {
                Ok(true)
            }
            fn input_text(
                &mut self,
                _prompt: &str,
                _allow_empty: bool,
            ) -> Result<String, dialoguer::Error> {
                Ok("hello".to_string())
            }
        }

        let mut io = ConsoleDialogIo::with_backend(InputBackend);
        let value = io
            .input_text("Prompt", false)
            .expect("input should succeed");
        assert_eq!(value, "hello");
    }

    #[test]
    fn dialog_io_input_text_default_returns_unsupported() {
        struct MinimalIo;
        impl DialogIo for MinimalIo {
            fn select_index(
                &mut self,
                _prompt: &str,
                _labels: &[String],
                _default: usize,
            ) -> Result<Option<usize>, std::io::Error> {
                Ok(None)
            }

            fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, std::io::Error> {
                Ok(false)
            }
        }

        let mut io = MinimalIo;
        let error = io
            .input_text("Prompt", true)
            .expect_err("default input_text should be unsupported");
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
        assert!(error.to_string().contains("not implemented"));
    }

    #[test]
    fn scripted_dialoguer_backend_returns_scripted_values() {
        scripted_reset();
        scripted_push_select(Ok(Some(1)));
        scripted_push_confirm(Ok(true));
        scripted_push_input(Ok("typed".to_string()));

        let mut io = ConsoleDialogIo::default();
        let labels = vec!["a".to_string(), "b".to_string()];

        assert_eq!(
            io.select_index("pick", &labels, 0)
                .expect("select should use scripted response"),
            Some(1)
        );
        assert!(io
            .confirm("confirm", false)
            .expect("confirm should use scripted response"));
        assert_eq!(
            io.input_text("prompt", true)
                .expect("input should use scripted response"),
            "typed"
        );
    }

    #[test]
    fn scripted_dialoguer_backend_errors_without_scripted_values() {
        scripted_reset();
        let mut backend = DialoguerBackend;

        let select_error = backend
            .select_index("pick", &[], 0)
            .expect_err("missing selection should error");
        assert!(select_error
            .to_string()
            .contains("no scripted selection response"));

        let confirm_error = backend
            .confirm("confirm", false)
            .expect_err("missing confirm should error");
        assert!(confirm_error
            .to_string()
            .contains("no scripted confirm response"));

        let input_error = backend
            .input_text("prompt", false)
            .expect_err("missing input should error");
        assert!(input_error
            .to_string()
            .contains("no scripted input response"));
    }
}
