use model::DatabaseFactory;
use std::error::Error;

pub(crate) fn with_savepoint<T>(
    database: &mut dyn DatabaseFactory,
    name: &str,
    operation: impl FnOnce(&mut dyn DatabaseFactory) -> Result<T, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    database.begin_savepoint(name)?;
    let outcome = operation(database).and_then(|value| {
        database.release_savepoint(name)?;
        Ok(value)
    });
    match outcome {
        Ok(value) => Ok(value),
        Err(operation_error) => {
            let rollback_error = database.rollback_to_savepoint(name).err();
            let release_error = database.release_savepoint(name).err();
            if rollback_error.is_none() && release_error.is_none() {
                return Err(operation_error);
            }

            let mut message = format!(
                "operation failed inside savepoint '{name}': {}",
                operation_error
            );
            if let Some(error) = rollback_error {
                message.push_str(&format!("; rollback failed: {error}"));
            }
            if let Some(error) = release_error {
                message.push_str(&format!("; release failed: {error}"));
            }
            Err(message.into())
        }
    }
}
