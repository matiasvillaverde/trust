//! Database operations for trade grades
//!
//! This module provides database operations for trade grade storage and retrieval,
//! including CRUD operations and filtering by various criteria.

use crate::error::IntoDomainModel;
use crate::schema::trade_grades;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{Grade, TradeGrade};
use std::error::Error;
use uuid::Uuid;

/// Worker for handling trade grade database operations
#[derive(Debug)]
pub struct WorkerTradeGrade;

impl WorkerTradeGrade {
    /// Create a new trade grade record
    pub fn create(
        connection: &mut SqliteConnection,
        trade_id: Uuid,
        overall_score: u8,
        overall_grade: Grade,
        process_score: u8,
        risk_score: u8,
        execution_score: u8,
        documentation_score: u8,
        recommendations: Vec<String>,
        graded_at: NaiveDateTime,
    ) -> Result<TradeGrade, Box<dyn Error>> {
        let id = Uuid::new_v4();
        let now = Utc::now().naive_utc();
        let recommendations_json = serde_json::to_string(&recommendations)?;

        let new_trade_grade = NewTradeGrade {
            id: id.to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: trade_id.to_string(),
            overall_score: overall_score as i32,
            overall_grade: overall_grade_to_string(overall_grade),
            process_score: process_score as i32,
            risk_score: risk_score as i32,
            execution_score: execution_score as i32,
            documentation_score: documentation_score as i32,
            recommendations: Some(recommendations_json),
            graded_at,
        };

        let grade = diesel::insert_into(trade_grades::table)
            .values(&new_trade_grade)
            .get_result::<TradeGradeSQLite>(connection)?
            .into_domain_model()?;

        Ok(grade)
    }

    /// Read a trade grade by ID
    pub fn read_by_id(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<TradeGrade, Box<dyn Error>> {
        let grade = trade_grades::table
            .filter(trade_grades::id.eq(id.to_string()))
            .filter(trade_grades::deleted_at.is_null())
            .first::<TradeGradeSQLite>(connection)?
            .into_domain_model()?;

        Ok(grade)
    }

    /// Read trade grade by trade ID
    pub fn read_by_trade_id(
        connection: &mut SqliteConnection,
        trade_id: Uuid,
    ) -> Result<TradeGrade, Box<dyn Error>> {
        let grade = trade_grades::table
            .filter(trade_grades::trade_id.eq(trade_id.to_string()))
            .filter(trade_grades::deleted_at.is_null())
            .first::<TradeGradeSQLite>(connection)?
            .into_domain_model()?;

        Ok(grade)
    }

    /// Read all grades for trades (optionally filtered by date range)
    pub fn read_all(
        connection: &mut SqliteConnection,
        days_back: Option<u32>,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        let mut query = trade_grades::table
            .filter(trade_grades::deleted_at.is_null())
            .into_boxed();

        if let Some(days) = days_back {
            let cutoff_date = Utc::now().naive_utc() - chrono::Duration::days(days.into());
            query = query.filter(trade_grades::graded_at.ge(cutoff_date));
        }

        let grades = query
            .order(trade_grades::graded_at.desc())
            .load::<TradeGradeSQLite>(connection)?
            .into_iter()
            .map(|g| g.into_domain_model())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(grades)
    }
}

/// Database model for trade grades
#[derive(Debug, Queryable, Identifiable, Insertable)]
#[diesel(table_name = trade_grades)]
struct TradeGradeSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    trade_id: String,
    overall_score: i32,
    overall_grade: String,
    process_score: i32,
    risk_score: i32,
    execution_score: i32,
    documentation_score: i32,
    recommendations: Option<String>,
    graded_at: NaiveDateTime,
}

/// Insertable model for new trade grades
#[derive(Debug, Insertable)]
#[diesel(table_name = trade_grades)]
struct NewTradeGrade {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    trade_id: String,
    overall_score: i32,
    overall_grade: String,
    process_score: i32,
    risk_score: i32,
    execution_score: i32,
    documentation_score: i32,
    recommendations: Option<String>,
    graded_at: NaiveDateTime,
}

impl IntoDomainModel<TradeGrade> for TradeGradeSQLite {
    fn into_domain_model(self) -> Result<TradeGrade, Box<dyn Error>> {
        let recommendations = if let Some(rec_str) = self.recommendations {
            serde_json::from_str(&rec_str)?
        } else {
            Vec::new()
        };

        Ok(TradeGrade {
            id: Uuid::parse_str(&self.id)?,
            trade_id: Uuid::parse_str(&self.trade_id)?,
            overall_score: self.overall_score as u8,
            overall_grade: string_to_overall_grade(&self.overall_grade)?,
            process_score: self.process_score as u8,
            risk_score: self.risk_score as u8,
            execution_score: self.execution_score as u8,
            documentation_score: self.documentation_score as u8,
            recommendations,
            graded_at: self.graded_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
        })
    }
}

/// Convert Grade enum to string for database storage
fn overall_grade_to_string(grade: Grade) -> String {
    match grade {
        Grade::A => "A".to_string(),
        Grade::B => "B".to_string(),
        Grade::C => "C".to_string(),
        Grade::D => "D".to_string(),
        Grade::F => "F".to_string(),
    }
}

/// Convert string to Grade enum from database
fn string_to_overall_grade(s: &str) -> Result<Grade, Box<dyn Error>> {
    match s {
        "A" => Ok(Grade::A),
        "B" => Ok(Grade::B),
        "C" => Ok(Grade::C),
        "D" => Ok(Grade::D),
        "F" => Ok(Grade::F),
        _ => Err(format!("Invalid grade string: {}", s).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use diesel::Connection;

    #[test]
    fn test_create_trade_grade() {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();

        // Run migrations first
        diesel::sql_query("
            CREATE TABLE trade_grades (
                id TEXT PRIMARY KEY NOT NULL,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                deleted_at TIMESTAMP,
                trade_id TEXT NOT NULL,
                overall_score INTEGER NOT NULL CHECK (overall_score >= 0 AND overall_score <= 100),
                overall_grade TEXT NOT NULL CHECK (overall_grade IN ('A', 'B', 'C', 'D', 'F')),
                process_score INTEGER NOT NULL CHECK (process_score >= 0 AND process_score <= 100),
                risk_score INTEGER NOT NULL CHECK (risk_score >= 0 AND risk_score <= 100),
                execution_score INTEGER NOT NULL CHECK (execution_score >= 0 AND execution_score <= 100),
                documentation_score INTEGER NOT NULL CHECK (documentation_score >= 0 AND documentation_score <= 100),
                recommendations TEXT,
                graded_at TIMESTAMP NOT NULL
            );
        ").execute(&mut conn).unwrap();

        let trade_id = Uuid::new_v4();
        let recommendations = vec![
            "Improve order timing".to_string(),
            "Add more context".to_string(),
        ];
        let graded_at = Utc::now().naive_utc();

        let result = WorkerTradeGrade::create(
            &mut conn,
            trade_id,
            87,
            Grade::B,
            90,
            95,
            80,
            75,
            recommendations.clone(),
            graded_at,
        );

        assert!(result.is_ok());
        let grade = result.unwrap();
        assert_eq!(grade.trade_id, trade_id);
        assert_eq!(grade.overall_score, 87);
        assert_eq!(grade.overall_grade, Grade::B);
        assert_eq!(grade.process_score, 90);
        assert_eq!(grade.risk_score, 95);
        assert_eq!(grade.execution_score, 80);
        assert_eq!(grade.documentation_score, 75);
        assert_eq!(grade.recommendations, recommendations);
    }

    #[test]
    fn test_read_by_trade_id() {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();

        // Run migrations first
        diesel::sql_query("
            CREATE TABLE trade_grades (
                id TEXT PRIMARY KEY NOT NULL,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                deleted_at TIMESTAMP,
                trade_id TEXT NOT NULL,
                overall_score INTEGER NOT NULL CHECK (overall_score >= 0 AND overall_score <= 100),
                overall_grade TEXT NOT NULL CHECK (overall_grade IN ('A', 'B', 'C', 'D', 'F')),
                process_score INTEGER NOT NULL CHECK (process_score >= 0 AND process_score <= 100),
                risk_score INTEGER NOT NULL CHECK (risk_score >= 0 AND risk_score <= 100),
                execution_score INTEGER NOT NULL CHECK (execution_score >= 0 AND execution_score <= 100),
                documentation_score INTEGER NOT NULL CHECK (documentation_score >= 0 AND documentation_score <= 100),
                recommendations TEXT,
                graded_at TIMESTAMP NOT NULL
            );
        ").execute(&mut conn).unwrap();

        let trade_id = Uuid::new_v4();
        let recommendations = vec!["Test recommendation".to_string()];
        let graded_at = Utc::now().naive_utc();

        // Create a grade first
        let created_grade = WorkerTradeGrade::create(
            &mut conn,
            trade_id,
            75,
            Grade::C,
            80,
            85,
            70,
            65,
            recommendations.clone(),
            graded_at,
        )
        .unwrap();

        // Now read it back
        let result = WorkerTradeGrade::read_by_trade_id(&mut conn, trade_id);
        assert!(result.is_ok());

        let read_grade = result.unwrap();
        assert_eq!(read_grade.id, created_grade.id);
        assert_eq!(read_grade.trade_id, trade_id);
        assert_eq!(read_grade.overall_score, 75);
        assert_eq!(read_grade.overall_grade, Grade::C);
        assert_eq!(read_grade.recommendations, recommendations);
    }

    #[test]
    fn test_read_all_grades() {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();

        // Run migrations first
        diesel::sql_query("
            CREATE TABLE trade_grades (
                id TEXT PRIMARY KEY NOT NULL,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                deleted_at TIMESTAMP,
                trade_id TEXT NOT NULL,
                overall_score INTEGER NOT NULL CHECK (overall_score >= 0 AND overall_score <= 100),
                overall_grade TEXT NOT NULL CHECK (overall_grade IN ('A', 'B', 'C', 'D', 'F')),
                process_score INTEGER NOT NULL CHECK (process_score >= 0 AND process_score <= 100),
                risk_score INTEGER NOT NULL CHECK (risk_score >= 0 AND risk_score <= 100),
                execution_score INTEGER NOT NULL CHECK (execution_score >= 0 AND execution_score <= 100),
                documentation_score INTEGER NOT NULL CHECK (documentation_score >= 0 AND documentation_score <= 100),
                recommendations TEXT,
                graded_at TIMESTAMP NOT NULL
            );
        ").execute(&mut conn).unwrap();

        // Create multiple grades
        let trade_id1 = Uuid::new_v4();
        let trade_id2 = Uuid::new_v4();
        let graded_at = Utc::now().naive_utc();

        WorkerTradeGrade::create(
            &mut conn,
            trade_id1,
            90,
            Grade::A,
            95,
            90,
            85,
            90,
            vec!["Excellent trade".to_string()],
            graded_at,
        )
        .unwrap();

        WorkerTradeGrade::create(
            &mut conn,
            trade_id2,
            60,
            Grade::D,
            65,
            70,
            55,
            50,
            vec!["Needs improvement".to_string()],
            graded_at,
        )
        .unwrap();

        // Read all grades
        let result = WorkerTradeGrade::read_all(&mut conn, None);
        assert!(result.is_ok());

        let grades = result.unwrap();
        assert_eq!(grades.len(), 2);

        // Should be ordered by graded_at descending
        assert!(grades.iter().any(|g| g.overall_grade == Grade::A));
        assert!(grades.iter().any(|g| g.overall_grade == Grade::D));
    }

    #[test]
    fn test_grade_string_conversion() {
        assert_eq!(overall_grade_to_string(Grade::A), "A");
        assert_eq!(overall_grade_to_string(Grade::B), "B");
        assert_eq!(overall_grade_to_string(Grade::C), "C");
        assert_eq!(overall_grade_to_string(Grade::D), "D");
        assert_eq!(overall_grade_to_string(Grade::F), "F");

        assert_eq!(string_to_overall_grade("A").unwrap(), Grade::A);
        assert_eq!(string_to_overall_grade("B").unwrap(), Grade::B);
        assert_eq!(string_to_overall_grade("C").unwrap(), Grade::C);
        assert_eq!(string_to_overall_grade("D").unwrap(), Grade::D);
        assert_eq!(string_to_overall_grade("F").unwrap(), Grade::F);
        assert!(string_to_overall_grade("X").is_err());
    }
}
