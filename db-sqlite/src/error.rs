//! Error types for database operations and conversions
//!
//! This module provides error types for handling database conversion failures
//! that can occur when mapping between database rows and domain models.

use std::error::Error;
use std::fmt;

/// Error type for database row to domain model conversions
#[derive(Debug)]
pub struct ConversionError {
    field: String,
    details: String,
}

impl ConversionError {
    /// Create a new conversion error
    pub fn new(field: impl Into<String>, details: impl Into<String>) -> Self {
        ConversionError {
            field: field.into(),
            details: details.into(),
        }
    }
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Conversion error for field '{}': {}",
            self.field, self.details
        )
    }
}

impl Error for ConversionError {}

/// Helper trait for converting SQLite models to domain models
pub trait IntoDomainModel<T> {
    /// Convert SQLite model to domain model, handling errors
    fn into_domain_model(self) -> Result<T, Box<dyn Error>>;
}

/// Helper trait for converting collections of SQLite models
pub trait IntoDomainModels<T> {
    /// Convert collection of SQLite models to domain models
    fn into_domain_models(self) -> Result<Vec<T>, Box<dyn Error>>;
}

impl<S, T> IntoDomainModels<T> for Vec<S>
where
    S: IntoDomainModel<T>,
{
    fn into_domain_models(self) -> Result<Vec<T>, Box<dyn Error>> {
        self.into_iter()
            .map(|item| item.into_domain_model())
            .collect()
    }
}
